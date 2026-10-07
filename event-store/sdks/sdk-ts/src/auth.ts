/**
 * Connection and call credentials (ADR-024, #302).
 *
 * The ADR-024 nginx gateway checks HTTP Basic Auth on every gRPC call and
 * answers `UNAUTHENTICATED` when it fails. These helpers add an
 * `authorization` header to every call (unary and streaming, including
 * `subscribe`) through a grpc-js interceptor, so it works on plaintext and
 * TLS channels alike, and mirror the Rust client (`sdk-rs` `ClientConfig`):
 *
 * - `Basic base64(user:password)`, a fixed `Bearer` token, or a token
 *   provider read per call (rotation without reconnecting)
 * - credentials are refused over plaintext to a non-loopback host unless
 *   `allowInsecureCredentials` is set
 * - secrets never appear in `toString`, `util.inspect`, `JSON.stringify` or
 *   error messages
 */

import { inspect } from "node:util";
import {
  ChannelCredentials,
  InterceptingCall,
  Metadata,
  credentials as grpcCredentials,
  status,
  type ChannelOptions,
  type Interceptor,
  type InterceptingListener,
  type ServiceError,
  type StatusObject,
} from "@grpc/grpc-js";

/** gRPC metadata key the gateway reads (HTTP/2 header names are lowercase). */
export const AUTHORIZATION = "authorization";

/**
 * Supplies a bearer token per call, without the `Bearer ` prefix.
 *
 * Called on every RPC, so return a cached token and refresh it in the
 * background (see {@link SharedToken}). A thrown error or rejected promise
 * fails the call with {@link UnauthenticatedError}; do not put the token in it.
 */
export type TokenProvider = () => string | Promise<string>;

/** A bearer token that can be replaced while clients are using it. */
export class SharedToken {
  #token: string;

  constructor(token: string) {
    this.#token = token;
  }

  /** Replace the token. Takes effect on the next call. */
  set(token: string): void {
    this.#token = token;
  }

  /** Use as a {@link TokenProvider}: `Credentials.tokenProvider(shared.provider)`. */
  readonly provider: TokenProvider = () => this.#token;

  toString(): string {
    return "SharedToken(<redacted>)";
  }
  toJSON(): string {
    return this.toString();
  }
  [inspect.custom](): string {
    return this.toString();
  }
}

type Source =
  | { kind: "basic"; username: string; password: string }
  | { kind: "bearer"; token: string }
  | { kind: "provider"; provider: TokenProvider };

/**
 * Credentials sent with every call. Secrets are held in private fields and
 * are redacted from every string form of this object.
 */
export class Credentials {
  readonly #source: Source;

  private constructor(source: Source) {
    this.#source = source;
  }

  /** `authorization: Basic base64(username:password)`, what the ADR-024 gateway expects. */
  static basic(username: string, password: string): Credentials {
    return new Credentials({ kind: "basic", username, password });
  }

  /** `authorization: Bearer <token>` with a fixed token. */
  static bearer(token: string): Credentials {
    return new Credentials({ kind: "bearer", token });
  }

  /** `authorization: Bearer <token>` with the token read per call. */
  static tokenProvider(provider: TokenProvider): Credentials {
    if (typeof provider !== "function") {
      throw new ConfigError("tokenProvider must be a function");
    }
    return new Credentials({ kind: "provider", provider });
  }

  /** `basic`, `bearer` or `provider`. */
  get kind(): Source["kind"] {
    return this.#source.kind;
  }

  toString(): string {
    const s = this.#source;
    if (s.kind === "basic") return `Credentials(basic, username=${JSON.stringify(s.username)}, password=<redacted>)`;
    if (s.kind === "bearer") return "Credentials(bearer, <redacted>)";
    return "Credentials(tokenProvider)";
  }
  toJSON(): string {
    return this.toString();
  }
  [inspect.custom](): string {
    return this.toString();
  }

  /**
   * Validate and build the header resolver. Errors never contain the secret.
   * @internal
   */
  static _resolver(creds: Credentials): () => string | Promise<string> {
    const s = creds.#source;
    if (s.kind === "basic") {
      if (typeof s.username !== "string" || typeof s.password !== "string") {
        throw new ConfigError("basic auth username and password must be strings");
      }
      if (s.username.includes(":")) {
        throw new ConfigError("basic auth username must not contain ':'");
      }
      const value = headerValue("Basic", Buffer.from(`${s.username}:${s.password}`, "utf8").toString("base64"));
      return () => value;
    }
    if (s.kind === "bearer") {
      const value = headerValue("Bearer", s.token);
      return () => value;
    }
    const provider = s.provider;
    return () => {
      const token = provider();
      return typeof token === "string" ? headerValue("Bearer", token) : token.then((t) => headerValue("Bearer", t));
    };
  }
}

/** Accepted wherever credentials are configured. */
export type AuthOption =
  | Credentials
  | { basic: { username: string; password: string } }
  | { bearerToken: string }
  | { tokenProvider: TokenProvider };

function toCredentials(auth: AuthOption): Credentials {
  if (auth instanceof Credentials) return auth;
  if ("basic" in auth) return Credentials.basic(auth.basic.username, auth.basic.password);
  if ("bearerToken" in auth) return Credentials.bearer(auth.bearerToken);
  if ("tokenProvider" in auth) return Credentials.tokenProvider(auth.tokenProvider);
  throw new ConfigError("auth must be Credentials, { basic }, { bearerToken } or { tokenProvider }");
}

function headerValue(scheme: string, credential: string): string {
  if (typeof credential !== "string" || credential.length === 0) {
    throw new ConfigError("credential must be a non-empty string");
  }
  // Visible ASCII and space only: no CR/LF header injection, nothing
  // grpc-js would reject or mangle.
  if (!/^[\x20-\x7e]+$/.test(credential)) {
    throw new ConfigError("credential contains characters not allowed in a header");
  }
  return `${scheme} ${credential}`;
}

/** Invalid client configuration (endpoint, TLS, credentials). Never contains a secret. */
export class ConfigError extends Error {
  constructor(message: string) {
    super(`invalid event store client config: ${message}`);
    this.name = "ConfigError";
  }
}

/**
 * The server (or the ADR-024 gateway) rejected the call's credentials, or a
 * token provider failed. Keeps the gRPC `ServiceError` shape, so code that
 * checks `err.code === status.UNAUTHENTICATED` keeps working.
 */
export class UnauthenticatedError extends Error implements ServiceError {
  readonly code = status.UNAUTHENTICATED;
  readonly details: string;
  readonly metadata: Metadata;
  /** The original gRPC error or token-provider failure. */
  readonly cause?: unknown;

  constructor(details: string, metadata: Metadata = new Metadata(), options?: { cause?: unknown }) {
    super(`event store rejected the credentials (UNAUTHENTICATED): ${details}`);
    this.name = "UnauthenticatedError";
    this.details = details;
    this.metadata = metadata;
    if (options && "cause" in options) this.cause = options.cause;
  }
}

/** True for a gRPC `UNAUTHENTICATED` error (including {@link UnauthenticatedError}). */
export function isUnauthenticated(err: unknown): boolean {
  return (err as { code?: unknown } | null)?.code === status.UNAUTHENTICATED;
}

/** Map a gRPC `UNAUTHENTICATED` error to {@link UnauthenticatedError}; anything else unchanged. */
export function mapGrpcError(err: unknown): unknown {
  if (err instanceof UnauthenticatedError || !isUnauthenticated(err)) return err;
  const e = err as Partial<ServiceError>;
  return new UnauthenticatedError(e.details ?? "", e.metadata ?? new Metadata(), { cause: err });
}

/** TLS settings. Server certificates are always verified. */
export interface TlsOptions {
  /** PEM CA bundle; defaults to the Node / OS trust store. */
  rootCerts?: Buffer;
  /** PEM client key and certificate for mutual TLS (both or neither). */
  privateKey?: Buffer;
  certChain?: Buffer;
  /**
   * Verify the server certificate against this name (and send it as SNI)
   * instead of the endpoint host. Use when connecting by IP or through a tunnel.
   */
  serverName?: string;
}

/** Connection options shared by the typed and runtime clients. */
export interface ConnectionOptions {
  /**
   * Custom channel credentials. Secure ones count as TLS for the
   * plaintext-credentials guard. Mutually exclusive with `tls`.
   */
  credentials?: ChannelCredentials;
  /** Use TLS. Implied (with defaults) by an `https://` endpoint. */
  tls?: TlsOptions | boolean;
  /** Credentials sent with every call. */
  auth?: AuthOption;
  /**
   * Allow sending credentials over plaintext to a non-loopback host. Default
   * false: plaintext credentials only go to `localhost` and loopback IPs. The
   * ADR-024 gateway is plaintext until #301; set this only on a trusted network.
   */
  allowInsecureCredentials?: boolean;
  /** Extra grpc-js channel options (and interceptors) passed through. */
  channelOptions?: ChannelOptions & { interceptors?: Interceptor[] };
}

/** What a client constructor needs: grpc-js target, channel credentials, options. */
export interface ResolvedConnection {
  target: string;
  tls: boolean;
  channelCredentials: ChannelCredentials;
  options: ChannelOptions & { interceptors: Interceptor[] };
}

/**
 * Resolve `address` and options. Endpoint forms:
 * - `host:port` - plaintext, or TLS when `tls`/secure `credentials` is set
 * - `http://host:port` - plaintext; combining it with TLS is an error
 * - `https://host:port` - TLS (default trust store unless `tls` says otherwise)
 * Other grpc-js targets (`dns:`, `unix:`, ...) pass through unchanged.
 */
export function resolveConnection(address: string, opts: ConnectionOptions = {}): ResolvedConnection {
  const raw = (address ?? "").trim();
  if (raw.length === 0) throw new ConfigError("endpoint is empty");
  if (hasUserinfo(raw)) {
    // Never echo the endpoint here: it contains a secret.
    throw new ConfigError("endpoint must not contain user:password@; use the auth option");
  }
  if (opts.tls && opts.credentials) {
    throw new ConfigError("set either tls or credentials, not both");
  }
  const tlsRequested = Boolean(opts.tls) || (opts.credentials?._isSecure() ?? false);

  let target = raw;
  let tls = tlsRequested;
  const m = /^([a-zA-Z][a-zA-Z0-9+.-]*):\/\/(.*)$/.exec(raw);
  if (m && GRPC_JS_SCHEMES.has(m[1]!.toLowerCase())) {
    // grpc-js resolver target (`dns:///host:port`, ...): passed through; the
    // plaintext guard below fails closed for it.
  } else if (m) {
    const scheme = m[1]!.toLowerCase();
    const rest = m[2]!;
    if (scheme === "http") {
      if (tlsRequested) {
        throw new ConfigError("endpoint uses http:// but TLS is configured; use https:// or a bare host:port");
      }
      target = rest;
      tls = false;
    } else if (scheme === "https") {
      if (opts.credentials && !opts.credentials._isSecure()) {
        throw new ConfigError("endpoint uses https:// but insecure credentials were given");
      }
      target = rest;
      tls = true;
    } else {
      throw new ConfigError(`unsupported endpoint scheme '${scheme}' (use http or https)`);
    }
    if (/[/?#]/.test(target)) throw new ConfigError(`endpoint '${raw}' must not have a path`);
    if (target.length === 0) throw new ConfigError(`endpoint '${raw}' has no host`);
  }

  const creds = opts.auth === undefined ? undefined : toCredentials(opts.auth);
  if (creds && !tls && !opts.allowInsecureCredentials) {
    const host = hostOf(target);
    if (host === undefined || !isLoopback(host)) {
      throw new ConfigError(
        `refusing to send credentials over plaintext to '${host ?? "this endpoint"}'; use https:// or allowInsecureCredentials: true`,
      );
    }
  }

  let channelCredentials: ChannelCredentials;
  const options: ChannelOptions & { interceptors: Interceptor[] } = {
    ...(opts.channelOptions ?? {}),
    interceptors: [...(opts.channelOptions?.interceptors ?? [])],
  };
  if (opts.credentials) {
    channelCredentials = opts.credentials;
  } else if (tls) {
    const t: TlsOptions = typeof opts.tls === "object" ? opts.tls : {};
    if (Boolean(t.privateKey) !== Boolean(t.certChain)) {
      throw new ConfigError("tls.privateKey and tls.certChain must be set together");
    }
    channelCredentials = grpcCredentials.createSsl(t.rootCerts ?? null, t.privateKey ?? null, t.certChain ?? null);
    if (t.serverName) {
      options["grpc.ssl_target_name_override"] = t.serverName;
      options["grpc.default_authority"] = t.serverName;
    }
  } else {
    channelCredentials = grpcCredentials.createInsecure();
  }
  if (creds) options.interceptors.unshift(authInterceptor(creds));
  return { target, tls, channelCredentials, options };
}

/**
 * Interceptor that adds the `authorization` header to every call, unary and
 * streaming. A failing token provider fails the call with
 * {@link UnauthenticatedError} before anything is sent.
 */
export function authInterceptor(auth: AuthOption): Interceptor {
  const resolve = Credentials._resolver(toCredentials(auth));
  return (options, nextCall) => {
    // While an async token is pending the call has not started: a cancel
    // must not let it start later, and must still deliver a status.
    let pending: InterceptingListener | undefined;
    let cancelled = false;
    let deadlineTimer: ReturnType<typeof setTimeout> | undefined;
    const finish = (listener: InterceptingListener, code: status, details: string) => {
      const st: StatusObject = { code, details, metadata: new Metadata() };
      listener.onReceiveStatus(st);
    };
    return new InterceptingCall(nextCall(options), {
      start(metadata, listener, next) {
        const fail = (cause: unknown) => {
          if (cancelled) return;
          pending = undefined;
          clearTimeout(deadlineTimer);
          // Provider errors may contain anything; only our own (secret-free)
          // validation messages are surfaced.
          finish(listener, status.UNAUTHENTICATED, cause instanceof ConfigError ? cause.message : "token provider failed");
        };
        let value: string | Promise<string>;
        try {
          value = resolve();
        } catch (err) {
          return fail(err);
        }
        if (typeof value === "string") {
          metadata.set(AUTHORIZATION, value);
          return next(metadata, listener);
        }
        pending = listener;
        // The call's deadline timer only starts with the underlying call, so
        // enforce the deadline ourselves while the token is pending.
        const deadline = options.deadline === undefined ? Infinity : new Date(options.deadline).getTime();
        if (Number.isFinite(deadline)) {
          deadlineTimer = setTimeout(() => {
            if (cancelled || !pending) return;
            cancelled = true;
            pending = undefined;
            finish(listener, status.DEADLINE_EXCEEDED, "Deadline exceeded while waiting for the token provider");
          }, Math.max(0, deadline - Date.now()));
        }
        value.then((v) => {
          if (cancelled) return;
          pending = undefined;
          clearTimeout(deadlineTimer);
          metadata.set(AUTHORIZATION, v);
          next(metadata, listener);
        }, fail);
      },
      cancel(next) {
        cancelled = true;
        clearTimeout(deadlineTimer);
        if (pending) {
          const l = pending;
          pending = undefined;
          finish(l, status.CANCELLED, "Cancelled on client");
        }
        next();
      },
    });
  };
}

/** grpc-js resolver schemes passed through unchanged. */
const GRPC_JS_SCHEMES = new Set(["dns", "unix", "unix-abstract", "ipv4", "ipv6", "vsock"]);

/** True when the authority part of `endpoint` has `userinfo@`. */
function hasUserinfo(endpoint: string): boolean {
  // Any '@': resolver targets (dns:///user:pass@host) put userinfo after the
  // slashes, and no valid event store endpoint contains one.
  return endpoint.includes("@");
}

/** Host of a `host:port` / `[v6]:port` / `host` target; undefined for other grpc-js target forms. */
function hostOf(target: string): string | undefined {
  if (/^[a-zA-Z][a-zA-Z0-9+.-]*:(?!\d+$)/.test(target) && !target.startsWith("[")) {
    // `dns:...`, `unix:...`, `ipv4:...` etc.: fail closed.
    return undefined;
  }
  if (target.startsWith("[")) {
    const end = target.indexOf("]");
    return end > 0 ? target.slice(1, end) : undefined;
  }
  const parts = target.split(":");
  if (parts.length > 2) return target; // bare IPv6 without brackets
  return parts[0] || undefined;
}

function isLoopback(host: string): boolean {
  const h = host.toLowerCase();
  if (h === "localhost" || h === "::1" || h === "0:0:0:0:0:0:0:1") return true;
  const v4 = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(h);
  return v4 !== null && v4.slice(1).every((o) => Number(o) <= 255) && Number(v4[1]) === 127;
}
