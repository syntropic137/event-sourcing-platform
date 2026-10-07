/**
 * Upcasters: migrate stored events to the schema the code knows (ADR-007, ADR-027).
 *
 * Stored events are immutable. When an event schema changes, bump its
 * `schemaVersion` and register an upcaster from the old version. Upcasters
 * run on the stored JSON payload before decoding. Same semantics as the Rust
 * SDK's `Upcasters`:
 *
 * ```typescript
 * const upcasters = new Upcasters().register('MoneyDeposited', 1, 2, (body) => ({
 *   ...body,
 *   currency: 'EUR',
 * }));
 * const adapter = new GrpcEventStoreAdapter({ serverAddress, tenantId, upcasters });
 * ```
 *
 * - A step maps one `(eventType, version)` to a newer version of the same
 *   type, or (`rename`) to another type. Steps chain until no step matches.
 * - Events without a matching step pass through untouched.
 * - The result is decoded by dispatching on the final type and version; a
 *   registered type at a version with no class is `UnknownEventVersionError`,
 *   never skipped.
 * - A step that throws, does not return a JSON object, or a chain that loops
 *   is `UpcastError`.
 */

import type { JsonObject } from '../types/common';
import { UpcastError } from './errors';

/** One upcasting step. May mutate and return the body it receives. */
export type UpcastFn = (body: JsonObject) => JsonObject;

/** Result of `Upcasters.upcast`. */
export interface Upcasted {
  eventType: string;
  eventVersion: number;
  payload: JsonObject;
}

interface Step {
  toType: string;
  toVersion: number;
  fn: UpcastFn;
}

/** Upper bound on chained steps; more means a rename cycle. */
export const MAX_UPCAST_STEPS = 64;

/** ADR-027 event type: non-empty printable ASCII without spaces. */
export function isValidEventType(eventType: string): boolean {
  return /^[\x21-\x7e]+$/.test(eventType);
}

/** Proto3 `0` is "unset" and means version 1 (ADR-027). */
export function normalizeEventVersion(eventVersion: number | undefined | null): number {
  const v = Number(eventVersion ?? 0);
  return Number.isFinite(v) && v >= 1 ? v : 1;
}

/** True for a plain JSON object (not an array, not null). */
export function isJsonObject(value: unknown): value is JsonObject {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/**
 * A set of upcasting steps keyed by `(eventType, fromVersion)`.
 *
 * `register` and `rename` return `this` so calls chain. Invalid
 * registrations throw (programming errors caught at startup).
 */
export class Upcasters {
  private readonly steps = new Map<string, Step>();

  /** Migrate `eventType` from `fromVersion` to a newer `toVersion`. */
  register(eventType: string, fromVersion: number, toVersion: number, fn: UpcastFn): this {
    if (!(toVersion > fromVersion)) {
      throw new Error(
        `upcaster for '${eventType}' must go to a newer version (${fromVersion} -> ${toVersion})`
      );
    }
    return this.insert(eventType, fromVersion, eventType, toVersion, fn);
  }

  /** Migrate `(fromType, fromVersion)` to another event type. */
  rename(
    fromType: string,
    fromVersion: number,
    toType: string,
    toVersion: number,
    fn: UpcastFn
  ): this {
    if (fromType === toType) {
      throw new Error(`rename of '${fromType}' must change the event type; use register`);
    }
    return this.insert(fromType, fromVersion, toType, toVersion, fn);
  }

  /** True if no step is registered. */
  isEmpty(): boolean {
    return this.steps.size === 0;
  }

  /** True if a step starts at `(eventType, eventVersion)`. */
  handles(eventType: string, eventVersion: number): boolean {
    return this.steps.has(key(eventType, normalizeEventVersion(eventVersion)));
  }

  /**
   * Run the chain from `(eventType, eventVersion)`; returns the final type,
   * version and payload (unchanged when no step matches).
   */
  upcast(eventType: string, eventVersion: number, payload: JsonObject): Upcasted {
    let ty = eventType;
    let version = normalizeEventVersion(eventVersion);
    let body: JsonObject = payload;
    if (this.steps.has(key(ty, version)) && !isJsonObject(body)) {
      throw new UpcastError(ty, version, 'stored payload is not a JSON object');
    }
    let count = 0;
    for (
      let step = this.steps.get(key(ty, version));
      step;
      step = this.steps.get(key(ty, version))
    ) {
      count += 1;
      if (count > MAX_UPCAST_STEPS) {
        throw new UpcastError(
          eventType,
          normalizeEventVersion(eventVersion),
          `more than ${MAX_UPCAST_STEPS} chained steps (rename cycle?)`
        );
      }
      let result: unknown;
      try {
        result = step.fn(body);
      } catch (err) {
        throw new UpcastError(
          ty,
          version,
          `step to v${step.toVersion} failed: ${(err as Error)?.message ?? String(err)}`,
          0,
          err instanceof Error ? err : undefined
        );
      }
      if (!isJsonObject(result)) {
        throw new UpcastError(ty, version, 'step did not return a JSON object');
      }
      body = result;
      ty = step.toType;
      version = step.toVersion;
    }
    return { eventType: ty, eventVersion: version, payload: body };
  }

  private insert(
    fromType: string,
    fromVersion: number,
    toType: string,
    toVersion: number,
    fn: UpcastFn
  ): this {
    if (!isValidEventType(fromType) || !isValidEventType(toType)) {
      throw new Error(`invalid event type in upcaster '${fromType}' -> '${toType}'`);
    }
    if (
      !Number.isInteger(fromVersion) ||
      !Number.isInteger(toVersion) ||
      fromVersion < 1 ||
      toVersion < 1
    ) {
      throw new Error('event versions are integers starting at 1');
    }
    const k = key(fromType, fromVersion);
    if (this.steps.has(k)) {
      throw new Error(`duplicate upcaster for '${fromType}' v${fromVersion}`);
    }
    this.steps.set(k, { toType, toVersion, fn });
    return this;
  }
}

function key(eventType: string, version: number): string {
  // Event types are printable ASCII without spaces, so a space cannot collide.
  return `${eventType} ${version}`;
}
