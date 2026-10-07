//! Upcasters: migrate stored events to the schema the code knows (ADR-007).
//!
//! Stored events are immutable. When an event schema changes, bump its
//! version and register an upcaster from the old version. Upcasters run on
//! the raw JSON payload *before* decoding, in
//! [`EventStoreRepository::load`](crate::repository::EventStoreRepository)
//! and in the [`ProjectionRunner`](crate::projection::ProjectionRunner)
//! (configure each with `with_upcasters`).
//!
//! ```
//! use event_sourcing_rust::prelude::*;
//! use serde_json::json;
//!
//! // MoneyDeposited v1 was {"amount": 5}; v2 adds a currency.
//! let upcasters = Upcasters::new().register("MoneyDeposited", 1, 2, |mut body| {
//!     body["currency"] = json!("EUR");
//!     Ok(body)
//! });
//!
//! let (ty, version, body) = upcasters
//!     .upcast("MoneyDeposited", 1, json!({"amount": 5}))
//!     .unwrap();
//! assert_eq!((ty.as_str(), version), ("MoneyDeposited", 2));
//! assert_eq!(body, json!({"amount": 5, "currency": "EUR"}));
//! ```
//!
//! Rules:
//!
//! * A step maps one `(event_type, version)` to a newer version of the same
//!   type, or ([`rename`](Upcasters::rename)) to another type. Steps chain
//!   until no step matches.
//! * Events without a matching step pass through untouched (no JSON parse).
//! * The result is decoded by dispatching on the final type and version; an
//!   event the decoder does not know is a typed error
//!   ([`Error::UnknownEventType`] / [`Error::UnknownEventVersion`]), never
//!   skipped.
//! * A step that fails, does not return a JSON object, or a chain that loops
//!   is [`Error::Upcast`].

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::event::{DomainEvent, SerializedEvent};
use crate::projection::RecordedEvent;
use crate::wire;

type StepFn = dyn Fn(Value) -> Result<Value> + Send + Sync;

#[derive(Clone)]
struct Step {
    to_type: String,
    to_version: u32,
    f: Arc<StepFn>,
}

/// Upper bound on chained steps; more means a rename cycle.
const MAX_STEPS: usize = 64;

/// A set of upcasting steps keyed by `(event_type, from_version)`.
///
/// Cheap to clone (steps are shared).
#[derive(Clone, Default)]
pub struct Upcasters {
    /// event_type -> from_version -> step
    steps: HashMap<String, HashMap<u32, Step>>,
}

impl fmt::Debug for Upcasters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut keys: Vec<_> = self
            .steps
            .iter()
            .flat_map(|(t, by_version)| {
                by_version
                    .iter()
                    .map(move |(v, s)| format!("{t} v{v} -> {} v{}", s.to_type, s.to_version))
            })
            .collect();
        keys.sort();
        f.debug_struct("Upcasters").field("steps", &keys).finish()
    }
}

impl Upcasters {
    /// No steps: every event passes through.
    pub fn new() -> Self {
        Self::default()
    }

    /// Migrate `event_type` from `from_version` to `to_version` (which must
    /// be greater) by transforming the JSON payload.
    ///
    /// # Panics
    ///
    /// If `to_version <= from_version`, `from_version` is 0, the type name
    /// is invalid, or a step from `(event_type, from_version)` already
    /// exists. These are programming errors caught at startup.
    pub fn register<F>(self, event_type: &str, from_version: u32, to_version: u32, f: F) -> Self
    where
        F: Fn(Value) -> Result<Value> + Send + Sync + 'static,
    {
        assert!(
            to_version > from_version,
            "upcaster for '{event_type}' must go to a newer version ({from_version} -> {to_version})"
        );
        self.insert(
            event_type,
            from_version,
            event_type,
            to_version,
            Arc::new(f),
        )
    }

    /// Migrate `(from_type, from_version)` to another event type.
    ///
    /// # Panics
    ///
    /// Like [`register`](Self::register); `from_type` must differ from
    /// `to_type`.
    pub fn rename<F>(
        self,
        from_type: &str,
        from_version: u32,
        to_type: &str,
        to_version: u32,
        f: F,
    ) -> Self
    where
        F: Fn(Value) -> Result<Value> + Send + Sync + 'static,
    {
        assert!(
            from_type != to_type,
            "rename of '{from_type}' must change the event type; use register"
        );
        self.insert(from_type, from_version, to_type, to_version, Arc::new(f))
    }

    fn insert(
        mut self,
        from_type: &str,
        from_version: u32,
        to_type: &str,
        to_version: u32,
        f: Arc<StepFn>,
    ) -> Self {
        assert!(
            wire::is_valid_event_type(from_type) && wire::is_valid_event_type(to_type),
            "invalid event type in upcaster '{from_type}' -> '{to_type}'"
        );
        assert!(
            from_version >= 1 && to_version >= 1,
            "event versions start at 1"
        );
        let by_version = self.steps.entry(from_type.to_string()).or_default();
        assert!(
            !by_version.contains_key(&from_version),
            "duplicate upcaster for '{from_type}' v{from_version}"
        );
        by_version.insert(
            from_version,
            Step {
                to_type: to_type.to_string(),
                to_version,
                f,
            },
        );
        self
    }

    fn step(&self, event_type: &str, event_version: u32) -> Option<&Step> {
        self.steps.get(event_type)?.get(&event_version)
    }

    /// True if no step is registered.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// True if a step starts at `(event_type, event_version)`.
    pub fn handles(&self, event_type: &str, event_version: u32) -> bool {
        self.step(event_type, wire::normalize_version(event_version))
            .is_some()
    }

    /// The type and version the chain from `(event_type, event_version)`
    /// ends at, without running any step. Steps are keyed by type and
    /// version only, so the target does not depend on the payload. `None`
    /// for a chain that loops.
    pub fn target(&self, event_type: &str, event_version: u32) -> Option<(String, u32)> {
        let mut ty = event_type;
        let mut version = wire::normalize_version(event_version);
        for _ in 0..=MAX_STEPS {
            match self.step(ty, version) {
                None => return Some((ty.to_string(), version)),
                Some(step) => {
                    ty = &step.to_type;
                    version = step.to_version;
                }
            }
        }
        None
    }

    /// Run the chain from `(event_type, event_version)`; returns the final
    /// type, version and payload (unchanged when no step matches).
    pub fn upcast(
        &self,
        event_type: &str,
        event_version: u32,
        payload: Value,
    ) -> Result<(String, u32, Value)> {
        let mut ty = event_type.to_string();
        let mut version = wire::normalize_version(event_version);
        let mut body = payload;
        // Steps may index the body as an object; never hand them anything
        // else (ADR-027: payloads are JSON objects).
        if self.step(&ty, version).is_some() && !body.is_object() {
            return Err(upcast_error(
                &ty,
                version,
                "stored payload is not a JSON object",
            ));
        }
        let mut steps = 0;
        while let Some(step) = self.step(&ty, version) {
            steps += 1;
            if steps > MAX_STEPS {
                return Err(upcast_error(
                    event_type,
                    event_version,
                    "more than 64 chained steps (rename cycle?)",
                ));
            }
            body = (step.f)(body).map_err(|err| {
                upcast_error(
                    &ty,
                    version,
                    &format!("step to v{} failed: {err}", step.to_version),
                )
            })?;
            if !body.is_object() {
                return Err(upcast_error(
                    &ty,
                    version,
                    "step did not return a JSON object",
                ));
            }
            ty.clone_from(&step.to_type);
            version = step.to_version;
        }
        Ok((ty, version, body))
    }

    /// Upcast and decode a stored payload as `E`.
    pub fn decode<E: DomainEvent>(
        &self,
        event_type: &str,
        event_version: u32,
        payload: &[u8],
    ) -> Result<E> {
        if !self.handles(event_type, event_version) {
            return E::from_payload(&SerializedEvent::new(event_type, event_version, payload));
        }
        let (ty, version, body) = self.upcast_bytes(event_type, event_version, payload)?;
        E::from_payload(&SerializedEvent::new(&ty, version, &body))
    }

    fn upcast_bytes(
        &self,
        event_type: &str,
        event_version: u32,
        payload: &[u8],
    ) -> Result<(String, u32, Vec<u8>)> {
        let value: Value =
            serde_json::from_slice(payload).map_err(|source| Error::EventDecode {
                event_type: event_type.to_string(),
                event_version: wire::normalize_version(event_version),
                source,
            })?;
        let (ty, version, body) = self.upcast(event_type, event_version, value)?;
        Ok((ty, version, serde_json::to_vec(&body)?))
    }

    /// Upcast a recorded event: type, version and payload are replaced by
    /// the chain's result. Borrowed (no copy, no parse) when no step applies.
    pub fn upcast_recorded<'a>(&self, event: &'a RecordedEvent) -> Result<Cow<'a, RecordedEvent>> {
        if !self.handles(&event.event_type, event.event_version) {
            return Ok(Cow::Borrowed(event));
        }
        wire::check_content_type(&event.event_type, &event.content_type)?;
        let (ty, version, body) =
            self.upcast_bytes(&event.event_type, event.event_version, &event.payload)?;
        let mut out = event.clone();
        out.event_type = ty;
        out.event_version = version;
        out.payload = body;
        Ok(Cow::Owned(out))
    }
}

fn upcast_error(event_type: &str, event_version: u32, reason: &str) -> Error {
    Error::Upcast {
        event_type: event_type.to_string(),
        event_version,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventSchema;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Deposited {
        amount: i64,
        currency: String,
    }

    impl EventSchema for Deposited {
        const EVENT_TYPE: &'static str = "Deposited";
        const EVENT_VERSION: u32 = 3;
    }

    fn chain() -> Upcasters {
        Upcasters::new()
            .rename("Credited", 1, "Deposited", 1, Ok)
            .register("Deposited", 1, 2, |mut v| {
                v["amount"] = json!(v["amt"].as_i64().unwrap_or(0));
                v.as_object_mut().unwrap().remove("amt");
                Ok(v)
            })
            .register("Deposited", 2, 3, |mut v| {
                v["currency"] = json!("EUR");
                Ok(v)
            })
    }

    #[test]
    fn v1_loads_as_latest_through_the_chain() {
        let e: Deposited = chain().decode("Deposited", 1, br#"{"amt":7}"#).unwrap();
        assert_eq!(
            e,
            Deposited {
                amount: 7,
                currency: "EUR".into()
            }
        );
        // Version 0 (unset) is version 1.
        let e: Deposited = chain().decode("Deposited", 0, br#"{"amt":7}"#).unwrap();
        assert_eq!(e.amount, 7);
        // Renamed type, then the version chain.
        let e: Deposited = chain().decode("Credited", 1, br#"{"amt":2}"#).unwrap();
        assert_eq!(e.amount, 2);
    }

    #[test]
    fn current_version_passes_through() {
        let e: Deposited = chain()
            .decode("Deposited", 3, br#"{"amount":1,"currency":"USD"}"#)
            .unwrap();
        assert_eq!(e.currency, "USD");
    }

    #[test]
    fn version_without_upcaster_is_a_typed_error() {
        let err = chain()
            .decode::<Deposited>("Deposited", 4, br#"{}"#)
            .unwrap_err();
        assert!(
            matches!(
                err,
                Error::UnknownEventVersion {
                    event_version: 4,
                    ..
                }
            ),
            "{err:?}"
        );
        let err = Upcasters::new()
            .decode::<Deposited>("Deposited", 2, br#"{}"#)
            .unwrap_err();
        assert!(matches!(err, Error::UnknownEventVersion { .. }), "{err:?}");
        let err = chain()
            .decode::<Deposited>("Withdrawn", 1, br#"{}"#)
            .unwrap_err();
        assert!(matches!(err, Error::UnknownEventType { .. }), "{err:?}");
    }

    #[test]
    fn failing_or_non_object_steps_are_errors() {
        let failing = Upcasters::new().register("E", 1, 2, |_| Err(Error::domain("bad data")));
        assert!(matches!(
            failing.upcast("E", 1, json!({})),
            Err(Error::Upcast { .. })
        ));
        let scalar = Upcasters::new().register("E", 1, 2, |_| Ok(json!(5)));
        assert!(matches!(
            scalar.upcast("E", 1, json!({})),
            Err(Error::Upcast { .. })
        ));
        let not_json = Upcasters::new().register("E", 1, 2, Ok);
        assert!(matches!(
            not_json.decode::<Deposited>("E", 1, b"not json"),
            Err(Error::EventDecode { .. })
        ));
    }

    #[test]
    fn non_object_input_never_reaches_a_step() {
        // The step indexes the body like an object; an array would panic.
        let err = chain()
            .decode::<Deposited>("Deposited", 2, b"[125]")
            .unwrap_err();
        assert!(matches!(err, Error::Upcast { .. }), "{err:?}");
        assert!(matches!(
            chain().upcast("Deposited", 2, json!("x")),
            Err(Error::Upcast { .. })
        ));
        // No step: the decoder rejects it (serde alone would accept [..]).
        let err = chain()
            .decode::<Deposited>("Deposited", 3, br#"[1,"USD"]"#)
            .unwrap_err();
        assert!(matches!(err, Error::EventDecode { .. }), "{err:?}");
    }

    #[test]
    fn rename_cycles_are_detected() {
        let cycle = Upcasters::new()
            .rename("A", 1, "B", 1, Ok)
            .rename("B", 1, "A", 1, Ok);
        assert!(matches!(
            cycle.upcast("A", 1, json!({})),
            Err(Error::Upcast { .. })
        ));
    }

    #[test]
    fn target_follows_the_chain_without_running_it() {
        let never = Upcasters::new()
            .rename("Credited", 1, "Deposited", 1, |_| panic!("ran"))
            .register("Deposited", 1, 2, |_| panic!("ran"));
        let t = |ty, v| never.target(ty, v);
        assert_eq!(t("Credited", 1), Some(("Deposited".into(), 2)));
        assert_eq!(t("Credited", 0), Some(("Deposited".into(), 2)));
        assert_eq!(t("Deposited", 2), Some(("Deposited".into(), 2)));
        assert_eq!(t("Other", 0), Some(("Other".into(), 1)));
        let cycle = Upcasters::new()
            .rename("A", 1, "B", 1, Ok)
            .rename("B", 1, "A", 1, Ok);
        assert_eq!(cycle.target("A", 1), None);
        // A chain of exactly MAX_STEPS steps is not a cycle.
        let long =
            (1..=MAX_STEPS as u32).fold(Upcasters::new(), |u, v| u.register("E", v, v + 1, Ok));
        assert_eq!(
            long.target("E", 1),
            Some(("E".into(), MAX_STEPS as u32 + 1))
        );
        assert!(long.upcast("E", 1, json!({})).is_ok());
    }

    #[test]
    #[should_panic(expected = "duplicate upcaster")]
    fn duplicate_steps_panic() {
        let _ = Upcasters::new()
            .register("E", 1, 2, Ok)
            .register("E", 1, 3, Ok);
    }

    #[test]
    #[should_panic(expected = "newer version")]
    fn downgrades_panic() {
        let _ = Upcasters::new().register("E", 2, 1, Ok);
    }
}
