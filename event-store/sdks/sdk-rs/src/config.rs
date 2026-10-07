//! Connection configuration: endpoint parsing, TLS, timeouts, keepalive,
//! credentials.
//!
//! ```no_run
//! # async fn demo() -> anyhow::Result<()> {
//! use std::time::Duration;
//! use eventstore_sdk_rs::{ClientConfig, TlsConfig};
//!
//! let store = ClientConfig::new("https://events.example.com:443")
//!     .tls(TlsConfig::new().ca_certificate_pem(std::fs::read("ca.pem")?))
//!     .basic_auth("app", std::env::var("ESP_GATEWAY_PASSWORD")?)
//!     .request_timeout(Duration::from_secs(10))
//!     .connect()
//!     .await?;
//! # Ok(()) }
//! ```

use std::fmt;
use std::net::IpAddr;
use std::time::Duration;

use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};

use crate::auth::{Credentials, TokenProvider};

/// Default bound on establishing a connection (TCP + TLS handshake).
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default deadline for unary RPCs and for opening a subscription.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Default HTTP/2 PING interval.
pub const DEFAULT_HTTP2_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// Default wait for a PING ack before the connection is considered dead.
pub const DEFAULT_HTTP2_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);
/// Default TCP keepalive idle time.
pub const DEFAULT_TCP_KEEPALIVE: Duration = Duration::from_secs(60);

/// TLS settings. Server certificates are always verified; there is no
/// "skip verification" switch.
#[derive(Clone, Default)]
pub struct TlsConfig {
    ca_pem: Vec<Vec<u8>>,
    system_roots: Option<bool>,
    domain: Option<String>,
    identity: Option<(Vec<u8>, Vec<u8>)>,
}

impl TlsConfig {
    /// Verify the server against the OS trust store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Trust this PEM CA certificate (may be called repeatedly). Setting a
    /// custom CA disables the OS trust store unless
    /// [`Self::with_system_roots`] is also set.
    pub fn ca_certificate_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.ca_pem.push(pem.into());
        self
    }

    /// Also trust the OS trust store (default: only when no custom CA is
    /// set).
    pub fn with_system_roots(mut self, enabled: bool) -> Self {
        self.system_roots = Some(enabled);
        self
    }

    /// Verify the server certificate against this name (and send it as SNI)
    /// instead of the endpoint's host. Use when connecting by IP or through a
    /// tunnel.
    pub fn domain_name(mut self, domain: impl Into<String>) -> Self {
        self.domain = Some(domain.into());
        self
    }

    /// Present a client certificate (mutual TLS). Both values are PEM.
    pub fn client_identity_pem(
        mut self,
        cert_pem: impl Into<Vec<u8>>,
        key_pem: impl Into<Vec<u8>>,
    ) -> Self {
        self.identity = Some((cert_pem.into(), key_pem.into()));
        self
    }

    fn to_tonic(&self, handshake_timeout: Option<Duration>) -> ClientTlsConfig {
        let mut cfg =
            ClientTlsConfig::new().ca_certificates(self.ca_pem.iter().map(Certificate::from_pem));
        if self.system_roots.unwrap_or(self.ca_pem.is_empty()) {
            cfg = cfg.with_native_roots();
        }
        if let Some(domain) = &self.domain {
            cfg = cfg.domain_name(domain.clone());
        }
        if let Some((cert, key)) = &self.identity {
            cfg = cfg.identity(Identity::from_pem(cert, key));
        }
        if let Some(t) = handshake_timeout {
            cfg = cfg.timeout(t);
        }
        cfg
    }
}

impl fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsConfig")
            .field("custom_cas", &self.ca_pem.len())
            .field(
                "system_roots",
                &self.system_roots.unwrap_or(self.ca_pem.is_empty()),
            )
            .field("domain", &self.domain)
            .field(
                "client_identity",
                &self.identity.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// How to reach an event store. Build with [`ClientConfig::new`], then call
/// [`ClientConfig::connect`].
///
/// Endpoint forms:
/// - `host:port` - plaintext, or TLS when [`Self::tls`] is set
/// - `http://host:port` - plaintext; combining it with [`Self::tls`] is an error
/// - `https://host:port` - TLS (OS trust store unless [`Self::tls`] says otherwise)
///
/// Timeout semantics: [`Self::request_timeout`] bounds each unary RPC and the
/// *opening* of a subscription, never the lifetime of a subscription stream.
/// Dead connections under a long-lived subscription are detected by HTTP/2
/// keepalive ([`Self::http2_keepalive`]).
#[derive(Clone)]
pub struct ClientConfig {
    endpoint: String,
    tls: Option<TlsConfig>,
    connect_timeout: Option<Duration>,
    request_timeout: Option<Duration>,
    http2_keepalive_interval: Option<Duration>,
    http2_keepalive_timeout: Duration,
    keepalive_while_idle: bool,
    tcp_keepalive: Option<Duration>,
    lazy: bool,
    credentials: Option<Credentials>,
    allow_insecure_credentials: bool,
}

impl ClientConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            tls: None,
            connect_timeout: Some(DEFAULT_CONNECT_TIMEOUT),
            request_timeout: Some(DEFAULT_REQUEST_TIMEOUT),
            http2_keepalive_interval: Some(DEFAULT_HTTP2_KEEPALIVE_INTERVAL),
            http2_keepalive_timeout: DEFAULT_HTTP2_KEEPALIVE_TIMEOUT,
            keepalive_while_idle: true,
            tcp_keepalive: Some(DEFAULT_TCP_KEEPALIVE),
            lazy: false,
            credentials: None,
            allow_insecure_credentials: false,
        }
    }

    /// Use TLS with these settings. Implied (with defaults) by `https://`.
    pub fn tls(mut self, tls: TlsConfig) -> Self {
        self.tls = Some(tls);
        self
    }

    /// Bound on establishing a connection, including the TLS handshake.
    /// `None` waits indefinitely.
    pub fn connect_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.connect_timeout = timeout.into();
        self
    }

    /// Deadline for each unary RPC (also sent to the server as
    /// `grpc-timeout`) and for opening a subscription. Never applied to a
    /// subscription stream once it is open. `None` disables it.
    pub fn request_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.request_timeout = timeout.into();
        self
    }

    /// HTTP/2 PING every `interval`; the connection is closed (failing its
    /// RPCs and subscriptions with `UNAVAILABLE`) if a PING is not acked
    /// within `timeout`. `None` disables HTTP/2 keepalive.
    pub fn http2_keepalive(
        mut self,
        interval: impl Into<Option<Duration>>,
        timeout: Duration,
    ) -> Self {
        self.http2_keepalive_interval = interval.into();
        self.http2_keepalive_timeout = timeout;
        self
    }

    /// PING even when hyper considers the connection idle (default true).
    ///
    /// Keep this on for subscriptions: hyper counts a connection whose only
    /// traffic is an open server stream as idle, so with `false` a dead
    /// connection under a quiet subscription is never detected (hyper 1.x).
    /// Turn it off only for servers that send GOAWAY on idle PINGs (for
    /// example grpc-go with a strict keepalive enforcement policy).
    pub fn keepalive_while_idle(mut self, enabled: bool) -> Self {
        self.keepalive_while_idle = enabled;
        self
    }

    /// TCP keepalive idle time. `None` disables it.
    pub fn tcp_keepalive(mut self, idle: impl Into<Option<Duration>>) -> Self {
        self.tcp_keepalive = idle.into();
        self
    }

    /// Return from [`Self::connect`] without connecting; the first RPC
    /// connects (and reconnects after failures). Endpoint and TLS settings
    /// are still validated up front.
    pub fn lazy_connect(mut self, lazy: bool) -> Self {
        self.lazy = lazy;
        self
    }

    pub fn credentials(mut self, credentials: Credentials) -> Self {
        self.credentials = Some(credentials);
        self
    }

    /// `authorization: Basic ...`, what the ADR-024 gateway expects.
    pub fn basic_auth(self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.credentials(Credentials::basic(username, password))
    }

    /// `authorization: Bearer <token>` with a fixed token.
    pub fn bearer_token(self, token: impl Into<String>) -> Self {
        self.credentials(Credentials::bearer(token))
    }

    /// `authorization: Bearer <token>` with the token read per RPC.
    pub fn token_provider(self, provider: impl TokenProvider) -> Self {
        self.credentials(Credentials::token_provider(provider))
    }

    /// Allow sending credentials over plaintext to a non-loopback host.
    /// Default false: credentials on plaintext are only sent to `localhost`
    /// and loopback IPs. The ADR-024 gateway is plaintext until #301 lands;
    /// set this only on a network you trust.
    pub fn allow_insecure_credentials(mut self, allow: bool) -> Self {
        self.allow_insecure_credentials = allow;
        self
    }

    pub(crate) fn request_timeout_value(&self) -> Option<Duration> {
        self.request_timeout
    }

    pub(crate) fn credentials_value(&self) -> Option<&Credentials> {
        self.credentials.as_ref()
    }

    pub(crate) fn connect_timeout_value(&self) -> Option<Duration> {
        self.connect_timeout
    }

    pub(crate) fn is_lazy(&self) -> bool {
        self.lazy
    }

    /// Resolve the endpoint URI and whether it uses TLS.
    pub(crate) fn resolve(&self) -> Result<ResolvedEndpoint, ConfigError> {
        let raw = self.endpoint.trim();
        if has_userinfo(raw) {
            // Never echo the endpoint here: it contains a secret.
            return Err(ConfigError::new(
                "endpoint must not contain user:password@; use basic_auth() or credentials()",
            ));
        }
        if raw.is_empty() {
            return Err(ConfigError::new("endpoint is empty"));
        }
        let (uri, tls) = match raw.split_once("://") {
            Some((scheme, rest)) => match scheme.to_ascii_lowercase().as_str() {
                "http" if self.tls.is_some() => return Err(ConfigError::new(
                    "endpoint uses http:// but TLS is configured; use https:// or a bare host:port",
                )),
                "http" => (format!("http://{rest}"), false),
                "https" => (format!("https://{rest}"), true),
                other => {
                    return Err(ConfigError::new(format!(
                        "unsupported endpoint scheme '{other}' (use http or https)"
                    )))
                }
            },
            None if self.tls.is_some() => (format!("https://{raw}"), true),
            None => (format!("http://{raw}"), false),
        };
        let parsed: tonic::codegen::http::Uri = uri
            .parse()
            .map_err(|e| ConfigError::new(format!("invalid endpoint '{raw}': {e}")))?;
        let host = parsed
            .host()
            .filter(|h| !h.is_empty())
            .ok_or_else(|| ConfigError::new(format!("endpoint '{raw}' has no host")))?
            .to_string();
        if parsed.path_and_query().is_some_and(|p| p.as_str() != "/") {
            return Err(ConfigError::new(format!(
                "endpoint '{raw}' must not have a path"
            )));
        }
        if !tls
            && self.credentials.is_some()
            && !self.allow_insecure_credentials
            && !is_loopback(&host)
        {
            return Err(ConfigError::new(format!(
                "refusing to send credentials over plaintext to '{host}'; use https:// or \
                 allow_insecure_credentials(true)"
            )));
        }
        Ok(ResolvedEndpoint { uri, tls })
    }

    /// Build the tonic endpoint. Every TLS endpoint has a TLS connector, and
    /// every endpoint with a TLS connector has an `https` URI (tonic would
    /// otherwise connect in plaintext).
    pub(crate) fn endpoint(&self) -> Result<Endpoint, ConfigError> {
        let resolved = self.resolve()?;
        let mut ep = Endpoint::from_shared(resolved.uri.clone())
            .map_err(|e| ConfigError::new(format!("invalid endpoint: {e}")))?
            .tcp_keepalive(self.tcp_keepalive)
            .keep_alive_timeout(self.http2_keepalive_timeout)
            .keep_alive_while_idle(self.keepalive_while_idle);
        if let Some(t) = self.connect_timeout {
            ep = ep.connect_timeout(t);
        }
        if let Some(i) = self.http2_keepalive_interval {
            ep = ep.http2_keep_alive_interval(i);
        }
        if resolved.tls {
            let tls = self.tls.clone().unwrap_or_default();
            ep = ep
                .tls_config(tls.to_tonic(self.connect_timeout))
                .map_err(|e| ConfigError::new(format!("invalid TLS config: {e}")))?;
        }
        Ok(ep)
    }
}

impl fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientConfig")
            .field("endpoint", &redact_userinfo(&self.endpoint))
            .field("tls", &self.tls)
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("http2_keepalive_interval", &self.http2_keepalive_interval)
            .field("http2_keepalive_timeout", &self.http2_keepalive_timeout)
            .field("keepalive_while_idle", &self.keepalive_while_idle)
            .field("tcp_keepalive", &self.tcp_keepalive)
            .field("lazy", &self.lazy)
            .field("credentials", &self.credentials)
            .field(
                "allow_insecure_credentials",
                &self.allow_insecure_credentials,
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedEndpoint {
    pub(crate) uri: String,
    pub(crate) tls: bool,
}

/// True when the authority part of `endpoint` has `userinfo@`.
fn has_userinfo(endpoint: &str) -> bool {
    let rest = endpoint.split_once("://").map_or(endpoint, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority.contains('@')
}

/// `endpoint` with any `userinfo@` replaced, for diagnostics.
fn redact_userinfo(endpoint: &str) -> String {
    if !has_userinfo(endpoint) {
        return endpoint.to_string();
    }
    let (scheme, rest) = match endpoint.split_once("://") {
        Some((s, r)) => (format!("{s}://"), r),
        None => (String::new(), endpoint),
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let at = rest[..authority_end].rfind('@').map_or(0, |i| i + 1);
    format!("{scheme}<redacted>@{}", &rest[at..])
}

fn is_loopback(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.eq_ignore_ascii_case("localhost")
        || bare.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Invalid client configuration (bad endpoint, TLS material, credentials).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(String);

impl ConfigError {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid event store client config: {}", self.0)
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(cfg: ClientConfig) -> Result<(String, bool), ConfigError> {
        cfg.resolve().map(|r| (r.uri, r.tls))
    }

    #[test]
    fn bare_host_port_is_plaintext() {
        assert_eq!(
            resolve(ClientConfig::new("127.0.0.1:50051")).unwrap(),
            ("http://127.0.0.1:50051".into(), false)
        );
    }

    #[test]
    fn http_url_kept_not_doubled() {
        assert_eq!(
            resolve(ClientConfig::new("http://es:50051")).unwrap(),
            ("http://es:50051".into(), false)
        );
    }

    #[test]
    fn https_url_means_tls_not_http_https() {
        assert_eq!(
            resolve(ClientConfig::new("https://es.example.com:443")).unwrap(),
            ("https://es.example.com:443".into(), true)
        );
        assert_eq!(
            resolve(ClientConfig::new("HTTPS://es:443")).unwrap(),
            ("https://es:443".into(), true)
        );
    }

    #[test]
    fn bare_host_with_tls_is_https() {
        assert_eq!(
            resolve(ClientConfig::new("es:443").tls(TlsConfig::new())).unwrap(),
            ("https://es:443".into(), true)
        );
    }

    #[test]
    fn http_with_tls_config_is_rejected() {
        // tonic would silently skip TLS for an http:// URI.
        assert!(resolve(ClientConfig::new("http://es:443").tls(TlsConfig::new())).is_err());
    }

    #[test]
    fn bad_endpoints_rejected() {
        for bad in [
            "",
            "   ",
            "grpc://es:1",
            "unix:///tmp/s",
            "http://",
            "http://es:1/path",
        ] {
            assert!(resolve(ClientConfig::new(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn https_endpoint_gets_tls_connector() {
        // Endpoint construction succeeds with TLS for https. Uses a custom CA
        // so the test does not depend on the host's trust store.
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let ca = params
            .self_signed(&rcgen::KeyPair::generate().unwrap())
            .unwrap();
        ClientConfig::new("https://es:443")
            .tls(TlsConfig::new().ca_certificate_pem(ca.pem()))
            .endpoint()
            .unwrap();
    }

    #[test]
    fn credentials_refused_over_plaintext_to_remote() {
        let err =
            resolve(ClientConfig::new("es.example.com:8081").basic_auth("u", "p")).unwrap_err();
        assert!(err.to_string().contains("plaintext"), "{err}");
        resolve(
            ClientConfig::new("es.example.com:8081")
                .basic_auth("u", "p")
                .allow_insecure_credentials(true),
        )
        .unwrap();
        resolve(ClientConfig::new("https://es.example.com").bearer_token("t")).unwrap();
        for local in ["localhost:1", "127.0.0.1:1", "[::1]:1"] {
            resolve(ClientConfig::new(local).bearer_token("t")).unwrap();
        }
    }

    #[test]
    fn keepalive_and_timeouts_applied_to_endpoint() {
        let ep = ClientConfig::new("127.0.0.1:1")
            .connect_timeout(Duration::from_secs(3))
            .tcp_keepalive(Duration::from_secs(7))
            .endpoint()
            .unwrap();
        assert_eq!(ep.get_connect_timeout(), Some(Duration::from_secs(3)));
        assert_eq!(ep.get_tcp_keepalive(), Some(Duration::from_secs(7)));
        let ep = ClientConfig::new("127.0.0.1:1")
            .connect_timeout(None)
            .tcp_keepalive(None)
            .endpoint()
            .unwrap();
        assert_eq!(ep.get_connect_timeout(), None);
        assert_eq!(ep.get_tcp_keepalive(), None);
    }

    #[test]
    fn userinfo_in_endpoint_rejected_and_redacted() {
        for ep in [
            "http://app:pw-secret@localhost:1",
            "app:pw-secret@localhost:1",
            "https://app:pw-secret@es:443/path",
        ] {
            let cfg = ClientConfig::new(ep);
            let err = cfg.resolve().unwrap_err().to_string();
            assert!(!err.contains("pw-secret"), "{err}");
            let dbg = format!("{cfg:?}");
            assert!(!dbg.contains("pw-secret"), "{dbg}");
        }
        assert_eq!(
            redact_userinfo("https://u:p@es:443/x"),
            "https://<redacted>@es:443/x"
        );
        assert!(!has_userinfo("https://es:443/a@b"));
    }

    #[test]
    fn debug_redacts() {
        let s = format!(
            "{:?}",
            ClientConfig::new("https://es")
                .basic_auth("user", "hunter2")
                .tls(TlsConfig::new().client_identity_pem("CERT", "PRIVATEKEY"))
        );
        assert!(!s.contains("hunter2") && !s.contains("PRIVATEKEY"), "{s}");
    }
}
