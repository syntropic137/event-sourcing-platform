/**
 * Connect-time correctness floor: ask the server what it guarantees.
 *
 * Servers from v0.17.0 implement GetServerInfo. Older servers answer it with
 * gRPC UNIMPLEMENTED, which maps to a "legacy" ServerInfo: unknown version,
 * no capabilities. Requirement checks therefore fail closed on old servers.
 */
import { status as GrpcStatus } from "@grpc/grpc-js";
import type { GetServerInfoResponse } from "./gen/eventstore/v1/eventstore.js";

/** First server version that implements GetServerInfo. */
export const SERVER_INFO_MIN_VERSION = "0.17.0";

/** Registry of capability flags. Names are stable and never reused. */
export const Capabilities = {
  /**
   * Global nonces become visible in commit order, so a reader paging ReadAll
   * or subscribing by global nonce never skips a late-committing nonce
   * (#337, v0.16.0).
   */
  COMMIT_ORDERED_GLOBAL_NONCE: "commit_ordered_global_nonce",
  /**
   * A subscription that cannot keep delivering ends with an error status
   * naming the resume position, never an empty result or silent end
   * (#350, v0.17.0).
   */
  SUBSCRIPTION_ERRORS_SURFACED: "subscription_errors_surfaced",
  /**
   * An undecodable stored event ends the subscription or read with DATA_LOSS
   * at its position; it is never skipped (#351, v0.17.0).
   */
  UNDECODABLE_EVENTS_SURFACED: "undecodable_events_surfaced",
  /**
   * The subscription aggregate id prefix is matched literally: `\`, `%` and
   * `_` are not wildcards (#361, v0.17.0).
   */
  LITERAL_SUBSCRIPTION_PREFIX: "literal_subscription_prefix",
} as const;

export interface ServerInfo {
  /** Server semver, e.g. "0.17.0". null when the server predates GetServerInfo. */
  serverVersion: string | null;
  /** Wire API, e.g. "eventstore.v1". null for legacy servers. */
  apiVersion: string | null;
  /** Backend kind ("memory", "postgres", ...). null for legacy servers. */
  backend: string | null;
  /** Capability flags the server guarantees. Empty for legacy servers. */
  capabilities: string[];
  /** True when the server answered UNIMPLEMENTED (older than SERVER_INFO_MIN_VERSION). */
  legacy: boolean;
}

export const LEGACY_SERVER_INFO: Readonly<ServerInfo> = Object.freeze({
  serverVersion: null,
  apiVersion: null,
  backend: null,
  capabilities: [],
  legacy: true,
});

export function fromServerInfoResponse(r: GetServerInfoResponse): ServerInfo {
  return {
    serverVersion: r.serverVersion,
    apiVersion: r.apiVersion,
    backend: r.backend,
    capabilities: [...r.capabilities],
    legacy: false,
  };
}

/** True for the gRPC status an older server returns for an unknown method. */
export function isUnimplemented(err: unknown): boolean {
  return (
    typeof err === "object" &&
    err !== null &&
    (err as { code?: unknown }).code === GrpcStatus.UNIMPLEMENTED
  );
}

/** The subset of `required` the server does not advertise, in order. */
export function missingCapabilities(info: ServerInfo, required: readonly string[]): string[] {
  return required.filter((c) => !info.capabilities.includes(c));
}

type PreId = bigint | string;

interface Semver {
  core: [bigint, bigint, bigint];
  pre: PreId[];
}

/**
 * Parse "MAJOR[.MINOR[.PATCH]][-PRE][+BUILD]" (leading "v" allowed, missing
 * minor/patch read as 0, build metadata ignored). BigInt avoids precision
 * loss on large numeric identifiers.
 */
function parseSemver(v: string): Semver | null {
  const s = v.trim().replace(/^v/, "").split("+")[0];
  const dash = s.indexOf("-");
  const core = dash >= 0 ? s.slice(0, dash) : s;
  const parts = core.split(".");
  if (parts.length > 3) return null;
  const nums = [parts[0], parts[1] ?? "0", parts[2] ?? "0"];
  if (!nums.every((p) => /^\d+$/.test(p))) return null;
  const pre: PreId[] = [];
  if (dash >= 0) {
    for (const id of s.slice(dash + 1).split(".")) {
      if (!/^[0-9A-Za-z-]+$/.test(id)) return null;
      pre.push(/^\d+$/.test(id) ? BigInt(id) : id);
    }
  }
  return { core: [BigInt(nums[0]), BigInt(nums[1]), BigInt(nums[2])], pre };
}

function cmp<T extends bigint | string>(a: T, b: T): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** SemVer 2.0 precedence: negative, zero, or positive. */
function compareSemver(a: Semver, b: Semver): number {
  for (let i = 0; i < 3; i++) {
    const c = cmp(a.core[i], b.core[i]);
    if (c !== 0) return c;
  }
  // A release outranks any of its pre-releases.
  if (a.pre.length === 0 && b.pre.length === 0) return 0;
  if (a.pre.length === 0) return 1;
  if (b.pre.length === 0) return -1;
  for (let i = 0; i < Math.min(a.pre.length, b.pre.length); i++) {
    const x = a.pre[i];
    const y = b.pre[i];
    const xNum = typeof x === "bigint";
    const yNum = typeof y === "bigint";
    // Numeric identifiers sort below alphanumeric ones.
    if (xNum !== yNum) return xNum ? -1 : 1;
    const c = xNum ? cmp(x as bigint, y as bigint) : cmp(x as string, y as string);
    if (c !== 0) return c;
  }
  // A shorter prefix sorts first.
  return a.pre.length - b.pre.length;
}

/**
 * True when the server version is known and >= `min` under SemVer 2.0
 * precedence (pre-releases compared identifier by identifier, build metadata
 * ignored). Always false for legacy servers, whose version is unknown.
 */
export function versionAtLeast(info: ServerInfo, min: string): boolean {
  if (info.serverVersion === null) return false;
  const have = parseSemver(info.serverVersion);
  const want = parseSemver(min);
  if (!have || !want) return false;
  return compareSemver(have, want) >= 0;
}

/** Thrown when the server does not meet a client's stated floor. */
export class CompatibilityError extends Error {
  constructor(
    message: string,
    readonly info: ServerInfo,
    readonly missing: string[] = [],
    readonly requiredVersion: string | null = null,
  ) {
    super(message);
    this.name = "CompatibilityError";
  }
}

function describeVersion(info: ServerInfo): string {
  return info.serverVersion ?? `< ${SERVER_INFO_MIN_VERSION} (no GetServerInfo)`;
}

export function assertCapabilities(info: ServerInfo, required: readonly string[]): void {
  const missing = missingCapabilities(info, required);
  if (missing.length > 0) {
    throw new CompatibilityError(
      `event store server ${describeVersion(info)} lacks required capabilities: ${missing.join(", ")}`,
      info,
      missing,
    );
  }
}

export function assertMinVersion(info: ServerInfo, min: string): void {
  if (!versionAtLeast(info, min)) {
    throw new CompatibilityError(
      `event store server ${describeVersion(info)} is older than required ${min}`,
      info,
      [],
      min,
    );
  }
}
