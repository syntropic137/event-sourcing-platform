//! Call credentials: an interceptor that adds an `authorization` header to
//! every RPC.
//!
//! The ADR-024 nginx gateway enforces HTTP Basic Auth (`auth_basic`) on its
//! external port, so it expects exactly `authorization: Basic
//! base64(user:password)`. Bearer tokens (`authorization: Bearer <token>`)
//! are for deployments that put a token-validating proxy in front of the
//! store. The event store itself does not check either header.
//!
//! Credential values are never printed: `Debug` impls redact them, and the
//! header is marked sensitive so HTTP/2 HPACK never adds it to the dynamic
//! table.

use std::fmt;
use std::sync::{Arc, RwLock};

use base64::Engine as _;
use tonic::metadata::{AsciiMetadataValue, MetadataValue};
use tonic::service::Interceptor;
use tonic::{Request, Status};

/// gRPC metadata key the gateway reads (HTTP/2 header names are lowercase).
pub const AUTHORIZATION: &str = "authorization";

/// Supplies a bearer token per RPC, so tokens can rotate without
/// reconnecting.
///
/// Called synchronously on every request (tonic interceptors are not async),
/// so it must be cheap and must not block: return a cached token and refresh
/// it from a background task. [`SharedToken`] is a ready-made implementation.
pub trait TokenProvider: Send + Sync + 'static {
    /// The current token, without the `Bearer ` prefix.
    ///
    /// An `Err` fails the RPC with that status before it is sent. Do not put
    /// the token in the error message.
    fn token(&self) -> Result<String, Status>;
}

/// A bearer token that can be replaced while clients are using it.
///
/// Clones share the token: hand one clone to [`crate::ClientConfig::token_provider`]
/// and call [`SharedToken::set`] on another from your refresh task.
#[derive(Clone)]
pub struct SharedToken {
    inner: Arc<RwLock<String>>,
}

impl SharedToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(token.into())),
        }
    }

    /// Replace the token. Takes effect on the next RPC.
    pub fn set(&self, token: impl Into<String>) {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *guard = token.into();
    }
}

impl TokenProvider for SharedToken {
    fn token(&self) -> Result<String, Status> {
        Ok(self.inner.read().unwrap_or_else(|e| e.into_inner()).clone())
    }
}

impl fmt::Debug for SharedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SharedToken(<redacted>)")
    }
}

/// Credentials sent with every RPC.
#[derive(Clone)]
pub enum Credentials {
    /// `authorization: Basic base64(username:password)`, what the ADR-024
    /// gateway expects.
    Basic { username: String, password: String },
    /// `authorization: Bearer <token>` with a fixed token.
    Bearer(String),
    /// `authorization: Bearer <token>` with the token read per RPC.
    TokenProvider(Arc<dyn TokenProvider>),
}

impl Credentials {
    pub fn basic(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    pub fn bearer(token: impl Into<String>) -> Self {
        Self::Bearer(token.into())
    }

    pub fn token_provider(provider: impl TokenProvider) -> Self {
        Self::TokenProvider(Arc::new(provider))
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Basic { username, .. } => f
                .debug_struct("Basic")
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
            Self::Bearer(_) => f.write_str("Bearer(<redacted>)"),
            Self::TokenProvider(_) => f.write_str("TokenProvider(..)"),
        }
    }
}

/// Header source resolved once at connect time.
#[derive(Clone)]
enum HeaderSource {
    None,
    Static(AsciiMetadataValue),
    Provider(Arc<dyn TokenProvider>),
}

/// Interceptor that adds the configured `authorization` header. A no-op when
/// no credentials are configured.
#[derive(Clone)]
pub(crate) struct AuthInterceptor {
    source: HeaderSource,
}

impl AuthInterceptor {
    /// Validate credentials and precompute static header values. Errors never
    /// contain the secret.
    pub(crate) fn new(credentials: Option<&Credentials>) -> Result<Self, InvalidCredentials> {
        let source = match credentials {
            None => HeaderSource::None,
            Some(Credentials::Basic { username, password }) => {
                if username.contains(':') {
                    return Err(InvalidCredentials(
                        "basic auth username must not contain ':'",
                    ));
                }
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{username}:{password}"));
                HeaderSource::Static(header_value("Basic", &encoded)?)
            }
            Some(Credentials::Bearer(token)) => {
                HeaderSource::Static(header_value("Bearer", token)?)
            }
            Some(Credentials::TokenProvider(p)) => HeaderSource::Provider(p.clone()),
        };
        Ok(Self { source })
    }
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let value = match &self.source {
            HeaderSource::None => return Ok(request),
            HeaderSource::Static(v) => v.clone(),
            HeaderSource::Provider(p) => header_value("Bearer", &p.token()?)
                .map_err(|e| Status::unauthenticated(e.to_string()))?,
        };
        request.metadata_mut().insert(AUTHORIZATION, value);
        Ok(request)
    }
}

impl fmt::Debug for AuthInterceptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.source {
            HeaderSource::None => "none",
            HeaderSource::Static(_) => "static",
            HeaderSource::Provider(_) => "provider",
        };
        f.debug_struct("AuthInterceptor")
            .field("source", &kind)
            .finish()
    }
}

fn header_value(scheme: &str, credential: &str) -> Result<AsciiMetadataValue, InvalidCredentials> {
    if credential.is_empty() {
        return Err(InvalidCredentials("credential must not be empty"));
    }
    let mut value: AsciiMetadataValue = MetadataValue::try_from(format!("{scheme} {credential}"))
        .map_err(|_| {
        InvalidCredentials("credential contains characters not allowed in a header")
    })?;
    value.set_sensitive(true);
    Ok(value)
}

/// Credentials that cannot be sent as a header. The message never contains
/// the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidCredentials(&'static str);

impl fmt::Display for InvalidCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid credentials: {}", self.0)
    }
}

impl std::error::Error for InvalidCredentials {}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(creds: &Credentials) -> Option<String> {
        let mut i = AuthInterceptor::new(Some(creds)).expect("valid");
        let req = i.call(Request::new(())).expect("intercept");
        req.metadata()
            .get(AUTHORIZATION)
            .map(|v| v.to_str().unwrap().to_string())
    }

    #[test]
    fn basic_matches_rfc7617() {
        // RFC 7617 example: Aladdin / open sesame.
        assert_eq!(
            header(&Credentials::basic("Aladdin", "open sesame")).as_deref(),
            Some("Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==")
        );
    }

    #[test]
    fn bearer_and_provider() {
        assert_eq!(
            header(&Credentials::bearer("t0k")).as_deref(),
            Some("Bearer t0k")
        );
        let shared = SharedToken::new("a");
        let creds = Credentials::token_provider(shared.clone());
        assert_eq!(header(&creds).as_deref(), Some("Bearer a"));
        shared.set("b");
        assert_eq!(header(&creds).as_deref(), Some("Bearer b"));
    }

    #[test]
    fn header_is_marked_sensitive() {
        let mut i = AuthInterceptor::new(Some(&Credentials::bearer("x"))).unwrap();
        let req = i.call(Request::new(())).unwrap();
        assert!(req.metadata().get(AUTHORIZATION).unwrap().is_sensitive());
    }

    #[test]
    fn none_adds_nothing() {
        let mut i = AuthInterceptor::new(None).unwrap();
        let req = i.call(Request::new(())).unwrap();
        assert!(req.metadata().get(AUTHORIZATION).is_none());
    }

    #[test]
    fn invalid_values_rejected_without_echoing_secret() {
        let err = AuthInterceptor::new(Some(&Credentials::bearer("sec\nret"))).unwrap_err();
        assert!(!err.to_string().contains("sec"));
        assert!(AuthInterceptor::new(Some(&Credentials::bearer(""))).is_err());
        assert!(AuthInterceptor::new(Some(&Credentials::basic("a:b", "p"))).is_err());
    }

    #[test]
    fn provider_error_fails_the_call() {
        struct Failing;
        impl TokenProvider for Failing {
            fn token(&self) -> Result<String, Status> {
                Err(Status::unauthenticated("token expired"))
            }
        }
        let mut i = AuthInterceptor::new(Some(&Credentials::token_provider(Failing))).unwrap();
        let err = i.call(Request::new(())).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn debug_redacts_secrets() {
        let s = format!(
            "{:?} {:?} {:?}",
            Credentials::basic("user", "hunter2"),
            Credentials::bearer("tok-secret"),
            SharedToken::new("shared-secret")
        );
        assert!(s.contains("user"));
        for secret in ["hunter2", "tok-secret", "shared-secret"] {
            assert!(!s.contains(secret), "{s}");
        }
    }
}
