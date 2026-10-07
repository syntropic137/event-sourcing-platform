import { test } from "node:test";
import assert from "node:assert/strict";
import { EventEmitter } from "node:events";

import { streamToAsyncIterator } from "./stream-iterator.js";

class FakeCall extends EventEmitter {
  cancelled = false;
  paused = false;
  cancel() {
    this.cancelled = true;
  }
  pause() {
    this.paused = true;
  }
  resume() {
    this.paused = false;
  }
}

const unavailable = Object.assign(new Error("14 UNAVAILABLE: db down"), { code: 14 });

/** Fails if `p` has not settled within `ms`: the old iterator hung here. */
function within<T>(p: Promise<T>, ms = 500): Promise<T> {
  return Promise.race([
    p,
    new Promise<T>((_, reject) =>
      setTimeout(() => reject(new Error(`next() did not settle within ${ms}ms`)), ms),
    ),
  ]);
}

test("error arriving while the consumer is busy rejects the next read", async () => {
  const call = new FakeCall();
  const it = streamToAsyncIterator<number>(call);

  const first = it.next();
  call.emit("data", 1);
  assert.deepEqual(await first, { value: 1, done: false });

  // Consumer is processing event 1: no next() pending.
  call.emit("data", 2);
  call.emit("error", unavailable);

  assert.deepEqual(await within(it.next()), { value: 2, done: false });
  await assert.rejects(within(it.next()), (e: any) => e.code === 14);
  assert.deepEqual(await within(it.next()), { value: undefined, done: true });
});

test("end arriving while the consumer is busy completes the next read", async () => {
  const call = new FakeCall();
  const it = streamToAsyncIterator<number>(call);
  call.emit("data", 1);
  call.emit("end");

  assert.deepEqual(await within(it.next()), { value: 1, done: false });
  assert.deepEqual(await within(it.next()), { value: undefined, done: true });
});

test("error while a read is pending rejects it", async () => {
  const call = new FakeCall();
  const it = streamToAsyncIterator<number>(call);
  const pending = it.next();
  call.emit("error", unavailable);
  await assert.rejects(within(pending), (e: any) => e.code === 14);
});

test("for-await surfaces the error after delivering buffered events", async () => {
  const call = new FakeCall();
  const it = streamToAsyncIterator<number>(call);
  call.emit("data", 1);
  call.emit("data", 2);
  call.emit("error", unavailable);

  const seen: number[] = [];
  await assert.rejects(
    within(
      (async () => {
        for await (const v of it) seen.push(v);
      })(),
    ),
    (e: any) => e.code === 14,
  );
  assert.deepEqual(seen, [1, 2]);
});

test("return() cancels the call and ignores the resulting CANCELLED error", async () => {
  const call = new FakeCall();
  const it = streamToAsyncIterator<number>(call);
  const pending = it.next();
  await it.return!();
  assert.equal(call.cancelled, true);
  assert.deepEqual(await pending, { value: undefined, done: true });
  call.emit("error", Object.assign(new Error("1 CANCELLED"), { code: 1 }));
  assert.deepEqual(await within(it.next()), { value: undefined, done: true });
});

test("a slow consumer pauses the call and resumes it after draining", async () => {
  const call = new FakeCall();
  const it = streamToAsyncIterator<number>(call);
  for (let i = 0; i < 300; i++) call.emit("data", i);
  assert.equal(call.paused, true);
  for (let i = 0; i < 300; i++) {
    assert.deepEqual(await it.next(), { value: i, done: false });
  }
  assert.equal(call.paused, false);
});
