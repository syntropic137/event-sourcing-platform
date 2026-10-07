//! Connect-time correctness floor: ask the server what it guarantees.
//!
//! Servers from v0.17.0 implement `GetServerInfo`. Older servers answer it
//! with `UNIMPLEMENTED`, which this module maps to [`ServerInfo::legacy`]:
//! a server of unknown version that advertises no capabilities. Requirement
//! checks therefore fail closed against old servers.

use std::fmt;

use eventstore_proto::gen::{GetServerInfoRequest, GetServerInfoResponse};
use tonic::Code;

use crate::EventStore;

pub use eventstore_proto::capabilities;
pub use eventstore_proto::SERVER_INFO_MIN_VERSION;

/// What the server reported about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// Server semver, e.g. "0.17.0". `None` when the server predates
    /// `GetServerInfo` (older than [`SERVER_INFO_MIN_VERSION`]).
    pub server_version: Option<String>,
    /// Wire API, e.g. "eventstore.v1". `None` for legacy servers.
    pub api_version: Option<String>,
    /// Backend kind ("memory", "postgres", ...). `None` for legacy servers.
    pub backend: Option<String>,
    /// Capability flags the server guarantees. Empty for legacy servers.
    pub capabilities: Vec<String>,
}

impl ServerInfo {
    /// Info for a server that answered `GetServerInfo` with `UNIMPLEMENTED`.
    pub fn legacy() -> Self {
        Self {
            server_version: None,
            api_version: None,
            backend: None,
            capabilities: Vec::new(),
        }
    }

    /// True when the server predates `GetServerInfo`.
    pub fn is_legacy(&self) -> bool {
        self.server_version.is_none()
    }

    /// True when the server advertises `name`.
    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|c| c == name)
    }

    /// The subset of `required` the server does not advertise, in order.
    pub fn missing_capabilities(&self, required: &[&str]) -> Vec<String> {
        required
            .iter()
            .filter(|r| !self.has_capability(r))
            .map(|r| r.to_string())
            .collect()
    }

    /// True when the server version is known and `>= min` (numeric
    /// major.minor.patch; a pre-release suffix sorts below its release).
    /// Always false for legacy servers, since their version is unknown.
    pub fn version_at_least(&self, min: &str) -> bool {
        match (
            self.server_version.as_deref().and_then(parse_semver),
            parse_semver(min),
        ) {
            (Some(have), Some(want)) => have >= want,
            _ => false,
        }
    }
}

impl From<GetServerInfoResponse> for ServerInfo {
    fn from(r: GetServerInfoResponse) -> Self {
        Self {
            server_version: Some(r.server_version),
            api_version: Some(r.api_version),
            backend: Some(r.backend),
            capabilities: r.capabilities,
        }
    }
}

/// Returned when the server does not meet a client's stated floor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatibilityError {
    /// The server does not advertise these capabilities.
    MissingCapabilities {
        server_version: Option<String>,
        missing: Vec<String>,
    },
    /// The server version is older than required, or unknown (legacy).
    VersionTooOld {
        server_version: Option<String>,
        required: String,
    },
}

impl fmt::Display for CompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let version = |v: &Option<String>| {
            v.clone()
                .unwrap_or_else(|| format!("< {SERVER_INFO_MIN_VERSION} (no GetServerInfo)"))
        };
        match self {
            Self::MissingCapabilities {
                server_version,
                missing,
            } => write!(
                f,
                "event store server {} lacks required capabilities: {}",
                version(server_version),
                missing.join(", ")
            ),
            Self::VersionTooOld {
                server_version,
                required,
            } => write!(
                f,
                "event store server {} is older than required {required}",
                version(server_version)
            ),
        }
    }
}

impl std::error::Error for CompatibilityError {}

impl EventStore {
    /// Ask the server for its version, backend, and capabilities.
    ///
    /// A server older than [`SERVER_INFO_MIN_VERSION`] answers
    /// `UNIMPLEMENTED`; that is returned as `Ok(ServerInfo::legacy())`, not an
    /// error. Any other failure is returned as an error.
    pub async fn server_info(&mut self) -> anyhow::Result<ServerInfo> {
        let req = self.unary(GetServerInfoRequest {});
        match crate::bounded(self.request_timeout, self.inner.get_server_info(req)).await {
            Ok(resp) => Ok(resp.into()),
            Err(status) if status.code() == Code::Unimplemented => Ok(ServerInfo::legacy()),
            Err(status) => Err(status.into()),
        }
    }

    /// Fail with [`CompatibilityError::MissingCapabilities`] unless the server
    /// advertises every capability in `required`. Legacy servers advertise
    /// none, so they fail any non-empty requirement.
    pub async fn require_capabilities(&mut self, required: &[&str]) -> anyhow::Result<ServerInfo> {
        let info = self.server_info().await?;
        let missing = info.missing_capabilities(required);
        if !missing.is_empty() {
            return Err(CompatibilityError::MissingCapabilities {
                server_version: info.server_version,
                missing,
            }
            .into());
        }
        Ok(info)
    }

    /// Fail with [`CompatibilityError::VersionTooOld`] unless the server
    /// reports a version `>= min`. Legacy servers always fail. Prefer
    /// [`Self::require_capabilities`]: a capability names the guarantee you
    /// depend on rather than the release that happened to ship it.
    pub async fn require_min_version(&mut self, min: &str) -> anyhow::Result<ServerInfo> {
        let info = self.server_info().await?;
        if !info.version_at_least(min) {
            return Err(CompatibilityError::VersionTooOld {
                server_version: info.server_version,
                required: min.to_string(),
            }
            .into());
        }
        Ok(info)
    }
}

/// An arbitrary-size non-negative integer kept as digits. SemVer puts no
/// bound on numeric identifiers, so a fixed-width parse would reject valid
/// versions. Derived `Ord` (digit count, then digits) is numeric order once
/// leading zeros are stripped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Numeric {
    len: usize,
    digits: String,
}

fn numeric(s: &str) -> Option<Numeric> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let trimmed = s.trim_start_matches('0');
    let digits = if trimmed.is_empty() { "0" } else { trimmed };
    Some(Numeric {
        len: digits.len(),
        digits: digits.to_string(),
    })
}

/// One dot-separated pre-release identifier. Derived `Ord` matches SemVer
/// 2.0: numeric identifiers sort below alphanumeric ones, numerics compare
/// numerically, alphanumerics compare in ASCII order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum PreId {
    Num(Numeric),
    Alpha(String),
}

/// A parsed "MAJOR[.MINOR[.PATCH]][-PRE][+BUILD]" version (leading "v"
/// allowed, missing minor/patch read as 0, build metadata ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Semver {
    core: (Numeric, Numeric, Numeric),
    pre: Vec<PreId>,
}

impl Ord for Semver {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        self.core.cmp(&other.core).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                // A release outranks any of its pre-releases.
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                // Identifier-wise; a shorter prefix sorts first.
                (false, false) => self.pre.cmp(&other.pre),
            }
        })
    }
}

impl PartialOrd for Semver {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_semver(v: &str) -> Option<Semver> {
    let v = v.trim();
    let v = v.strip_prefix('v').unwrap_or(v);
    let v = v.split('+').next()?;
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let mut parts = core.split('.');
    let major = numeric(parts.next()?)?;
    let minor = numeric(parts.next().unwrap_or("0"))?;
    let patch = numeric(parts.next().unwrap_or("0"))?;
    if parts.next().is_some() {
        return None;
    }
    let pre = match pre {
        None => Vec::new(),
        Some(pre) => pre
            .split('.')
            .map(|id| {
                if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    None
                } else if id.chars().all(|c| c.is_ascii_digit()) {
                    numeric(id).map(PreId::Num)
                } else {
                    Some(PreId::Alpha(id.to_string()))
                }
            })
            .collect::<Option<Vec<_>>>()?,
    };
    Some(Semver {
        core: (major, minor, patch),
        pre,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reported(version: &str, caps: &[&str]) -> ServerInfo {
        GetServerInfoResponse {
            server_version: version.into(),
            api_version: "eventstore.v1".into(),
            backend: "postgres".into(),
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
        }
        .into()
    }

    #[test]
    fn legacy_has_no_capabilities_and_no_version() {
        let info = ServerInfo::legacy();
        assert!(info.is_legacy());
        assert!(!info.has_capability(capabilities::COMMIT_ORDERED_GLOBAL_NONCE));
        assert_eq!(
            info.missing_capabilities(&[capabilities::COMMIT_ORDERED_GLOBAL_NONCE]),
            vec![capabilities::COMMIT_ORDERED_GLOBAL_NONCE.to_string()]
        );
        assert!(!info.version_at_least("0.0.1"));
    }

    #[test]
    fn missing_capabilities_reports_only_absent_ones() {
        let info = reported("0.17.0", &[capabilities::COMMIT_ORDERED_GLOBAL_NONCE]);
        assert!(!info.is_legacy());
        assert!(info
            .missing_capabilities(&[capabilities::COMMIT_ORDERED_GLOBAL_NONCE])
            .is_empty());
        assert_eq!(
            info.missing_capabilities(&["future_flag", capabilities::COMMIT_ORDERED_GLOBAL_NONCE]),
            vec!["future_flag".to_string()]
        );
    }

    #[test]
    fn version_comparison() {
        let info = reported("0.17.0", &[]);
        assert!(info.version_at_least("0.16.0"));
        assert!(info.version_at_least("0.17.0"));
        assert!(info.version_at_least("v0.17"));
        assert!(info.version_at_least("0.17.0-rc.1"));
        assert!(!info.version_at_least("0.17.1"));
        assert!(!info.version_at_least("1.0.0"));
        assert!(!info.version_at_least("garbage"));
        assert!(!reported("0.17.0-rc.1", &[]).version_at_least("0.17.0"));
        assert!(reported("0.18.0+abc", &[]).version_at_least("0.17.0"));
        assert!(!reported("", &[]).version_at_least("0.0.0"));
    }

    #[test]
    fn prerelease_precedence_follows_semver() {
        // SemVer 2.0 section 11 example, ascending.
        let ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for (i, have) in ordered.iter().enumerate() {
            let info = reported(have, &[]);
            for (j, want) in ordered.iter().enumerate() {
                assert_eq!(
                    info.version_at_least(want),
                    i >= j,
                    "{have} >= {want} should be {}",
                    i >= j
                );
            }
        }
        // Distinct pre-releases no longer collapse to one rank.
        assert!(!reported("0.17.0-alpha.1", &[]).version_at_least("0.17.0-rc.2"));
        assert!(reported("0.17.0-rc.2", &[]).version_at_least("0.17.0-rc.1"));
        assert!(!reported("0.17.0-rc.1", &[]).version_at_least("0.17.0-rc.2"));
        assert!(reported("0.17.0-rc.10", &[]).version_at_least("0.17.0-rc.9"));
        assert!(reported("0.17.0-rc.2+build.5", &[]).version_at_least("0.17.0-rc.2"));
        assert!(!reported("0.17.0-rc..1", &[]).version_at_least("0.0.0"));
        assert!(!reported("0.17.0-", &[]).version_at_least("0.0.0"));
        // Numeric identifiers beyond u64 still compare numerically.
        assert!(reported("1.0.0", &[]).version_at_least("1.0.0-rc.18446744073709551616"));
        assert!(reported("1.0.0-rc.18446744073709551617", &[])
            .version_at_least("1.0.0-rc.18446744073709551616"));
        assert!(!reported("1.0.0-rc.18446744073709551616", &[])
            .version_at_least("1.0.0-rc.18446744073709551617"));
        assert!(reported("1.0.0-rc.99999999999999999999", &[]).version_at_least("1.0.0-rc.9"));
        assert!(
            reported("18446744073709551616.0.0", &[]).version_at_least("18446744073709551615.9.9")
        );
        assert!(!reported("1.0.0-rc.99999999999999999999", &[]).version_at_least("1.0.0-rc.a"));
    }

    #[test]
    fn compatibility_error_messages_name_the_gap() {
        let e = CompatibilityError::MissingCapabilities {
            server_version: None,
            missing: vec!["commit_ordered_global_nonce".into()],
        };
        let msg = e.to_string();
        assert!(msg.contains("commit_ordered_global_nonce"), "{msg}");
        assert!(msg.contains("no GetServerInfo"), "{msg}");
    }
}
