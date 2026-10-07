// Generated server & client will be included via tonic
pub mod gen {
    tonic::include_proto!("eventstore.v1");
}

/// Wire API identifier reported in `GetServerInfoResponse.api_version`.
pub const API_VERSION: &str = "eventstore.v1";

/// First server version that implements `GetServerInfo`. A server that
/// answers that RPC with `UNIMPLEMENTED` is older than this and must be
/// treated as advertising no capabilities.
pub const SERVER_INFO_MIN_VERSION: &str = "0.17.0";

/// Registry of capability flags reported in `GetServerInfoResponse.capabilities`.
///
/// A capability names a behavioral guarantee, not a feature toggle. Names are
/// stable once released and never reused. See the event store compatibility
/// docs for the server version that introduced each one.
pub mod capabilities {
    /// Global nonces become visible to readers in commit order, so a reader
    /// paging `ReadAll` or subscribing by global nonce never skips a nonce
    /// that commits after the cursor has passed it (#337, v0.16.0).
    pub const COMMIT_ORDERED_GLOBAL_NONCE: &str = "commit_ordered_global_nonce";
}
