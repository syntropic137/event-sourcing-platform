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

type SemverKey = [number, number, number, number];

function parseSemver(v: string): SemverKey | null {
  const s = v.trim().replace(/^v/, "").split("+")[0];
  const dash = s.indexOf("-");
  const core = dash >= 0 ? s.slice(0, dash) : s;
  const release = dash >= 0 ? 0 : 1; // pre-release sorts below its release
  const parts = core.split(".");
  if (parts.length === 0 || parts.length > 3) return null;
  const nums = [parts[0], parts[1] ?? "0", parts[2] ?? "0"].map((p) =>
    /^\d+$/.test(p) ? Number(p) : NaN,
  );
  if (nums.some(Number.isNaN)) return null;
  return [nums[0], nums[1], nums[2], release];
}

/**
 * True when the server version is known and >= `min` (numeric
 * major.minor.patch; a pre-release sorts below its release). Always false for
 * legacy servers, whose version is unknown.
 */
export function versionAtLeast(info: ServerInfo, min: string): boolean {
  if (info.serverVersion === null) return false;
  const have = parseSemver(info.serverVersion);
  const want = parseSemver(min);
  if (!have || !want) return false;
  for (let i = 0; i < 4; i++) {
    if (have[i] !== want[i]) return have[i] > want[i];
  }
  return true;
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
