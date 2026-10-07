/**
 * The subset of a gRPC server-streaming call (`ClientReadableStream`) used by
 * {@link streamToAsyncIterator}. Kept structural so it can be tested without
 * a server.
 */
export interface ReadableCall {
  on(event: "data", listener: (data: any) => void): unknown;
  on(event: "error", listener: (err: any) => void): unknown;
  on(event: "end", listener: () => void): unknown;
  cancel(): void;
  pause?(): unknown;
  resume?(): unknown;
}

/** Pause the call when this many messages are buffered and unread. */
const HIGH_WATER_MARK = 256;

type Waiter<T> = {
  resolve: (r: IteratorResult<T>) => void;
  reject: (e: unknown) => void;
};

/**
 * Adapt a server-streaming call to an async iterator without losing messages.
 *
 * Listeners are attached once, for the whole call. Messages that arrive while
 * the consumer is busy (between `next()` calls) are buffered, and a terminal
 * `error` or `end` is remembered and delivered after the buffered messages.
 * So a subscription that fails while the consumer is processing an event
 * still rejects the next `next()` with the gRPC error (e.g. `UNAVAILABLE`,
 * `DATA_LOSS`) instead of hanging forever, and the consumer can reconnect
 * from its checkpoint (ADR-026).
 */
export function streamToAsyncIterator<T>(call: ReadableCall): AsyncIterableIterator<T> {
  const buffer: T[] = [];
  const waiters: Waiter<T>[] = [];
  let terminal: { kind: "end" } | { kind: "error"; error: unknown } | undefined;
  let finished = false; // return() called or terminal already delivered
  let paused = false;

  const done = (): IteratorResult<T> => ({ value: undefined, done: true });

  const maybeResume = () => {
    if (paused && buffer.length < HIGH_WATER_MARK / 2) {
      paused = false;
      call.resume?.();
    }
  };

  /** Hand buffered messages, then the terminal outcome, to pending next() calls. */
  const settle = () => {
    while (waiters.length > 0 && buffer.length > 0) {
      waiters.shift()!.resolve({ value: buffer.shift() as T, done: false });
    }
    maybeResume();
    if (waiters.length > 0 && terminal && buffer.length === 0) {
      finished = true;
      const first = waiters.shift()!;
      if (terminal.kind === "error") first.reject(terminal.error);
      else first.resolve(done());
      while (waiters.length > 0) waiters.shift()!.resolve(done());
    }
  };

  call.on("data", (data: T) => {
    if (finished) return;
    buffer.push(data);
    if (!paused && buffer.length >= HIGH_WATER_MARK && call.pause) {
      paused = true;
      call.pause();
    }
    settle();
  });
  call.on("error", (error: unknown) => {
    if (finished || terminal) return;
    terminal = { kind: "error", error };
    settle();
  });
  call.on("end", () => {
    if (finished || terminal) return;
    terminal = { kind: "end" };
    settle();
  });

  const iterator: AsyncIterableIterator<T> = {
    [Symbol.asyncIterator]() {
      return iterator;
    },
    next(): Promise<IteratorResult<T>> {
      if (buffer.length === 0 && finished) return Promise.resolve(done());
      return new Promise<IteratorResult<T>>((resolve, reject) => {
        waiters.push({ resolve, reject });
        settle();
      });
    },
    return(): Promise<IteratorResult<T>> {
      if (!finished) {
        finished = true;
        buffer.length = 0;
        if (!terminal) call.cancel();
        while (waiters.length > 0) waiters.shift()!.resolve(done());
      }
      return Promise.resolve(done());
    },
  };
  return iterator;
}
