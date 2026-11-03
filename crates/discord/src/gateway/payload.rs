//! Gateway payloads and opcodes.
//!
//! Every message on a gateway connection is a JSON object with an opcode (`op`),
//! optional event data (`d`), and — for dispatch events — a sequence number (`s`)
//! and an event name (`t`). This module models that envelope and builds the
//! outbound payloads the client sends: Identify (op 2), Resume (op 6), Heartbeat
//! (op 1) and Presence Update (op 3).
//!
//! Reference: `docs/gateway-events.md` (Payload Structure, Send Events — Identify,
//! Resume, Heartbeat, Update Presence), `docs/gateway.md` (Connecting,
//! Identifying, Resuming, Sending Heartbeats) and
//! `docs/opcodes-and-status-codes.md` (Gateway Opcodes).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{Error, GatewayError};
use crate::intents::GatewayIntents;

/// A gateway opcode.
///
/// Reference: `docs/opcodes-and-status-codes.md` (Gateway Opcodes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
pub enum Opcode {
    /// An event was dispatched.
    Dispatch = 0,
    /// A heartbeat, sent periodically by the client and sometimes requested by
    /// the server.
    Heartbeat = 1,
    /// Starts a new session during the initial handshake.
    Identify = 2,
    /// Updates the client's presence.
    PresenceUpdate = 3,
    /// Sent after connecting, carrying the heartbeat interval.
    Hello = 10,
    /// Acknowledge of a received heartbeat.
    HeartbeatAck = 11,
    /// The server asks the client to reconnect and resume.
    Reconnect = 7,
    /// The session is invalid; reconnect and identify or resume.
    InvalidSession = 9,
    /// Resumes a previous session.
    Resume = 6,
}

impl Opcode {
    /// Converts the raw `op` field from a payload into an opcode.
    ///
    /// Unknown opcodes return `None`; the caller logs and ignores them so a new
    /// Discord opcode cannot break the connection.
    #[must_use]
    pub const fn from_i64(value: i64) -> Option<Self> {
        match value {
            0 => Some(Self::Dispatch),
            1 => Some(Self::Heartbeat),
            2 => Some(Self::Identify),
            3 => Some(Self::PresenceUpdate),
            6 => Some(Self::Resume),
            7 => Some(Self::Reconnect),
            9 => Some(Self::InvalidSession),
            10 => Some(Self::Hello),
            11 => Some(Self::HeartbeatAck),
            _ => None,
        }
    }

    /// Returns the numeric opcode used on the wire.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        self as i64
    }
}

/// A decoded gateway payload.
///
/// `s` and `t` are `null` (therefore absent after deserialisation) when `op` is
/// not [`Opcode::Dispatch`].
///
/// Reference: `docs/gateway-events.md` (Payload Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayPayload {
    /// The gateway opcode.
    pub op: i64,
    /// Event data; its shape depends on the opcode and event name.
    #[serde(default)]
    pub d: Value,
    /// Sequence number of the event, used for heartbeats and resuming.
    #[serde(default)]
    pub s: Option<i64>,
    /// Event name, present on dispatch payloads.
    #[serde(default)]
    pub t: Option<String>,
}

/// Decodes one text frame from the gateway into a [`GatewayPayload`].
///
/// # Errors
///
/// Returns [`Error::Json`] when the frame is not a JSON payload.
pub fn decode(text: &str) -> Result<GatewayPayload, Error> {
    serde_json::from_str(text).map_err(Error::from)
}

/// The `d` object of a Hello (op 10) payload.
///
/// Reference: `docs/gateway-events.md` (Hello).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// Interval, in milliseconds, an app should heartbeats with.
    pub heartbeat_interval: i64,
}

impl Hello {
    /// Reads the Hello payload from an op 10 [`GatewayPayload`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Gateway`] with [`GatewayError::Protocol`] when the
    /// payload has no usable `heartbeat_interval` or a non-positive one.
    pub fn parse(payload: &GatewayPayload) -> Result<Self, Error> {
        let hello: Self = serde_json::from_value(payload.d.clone())?;

        if hello.heartbeat_interval <= 0 {
            return Err(Error::Gateway(GatewayError::Protocol(format!(
                "Hello contained an invalid heartbeat interval: {}",
                hello.heartbeat_interval
            ))));
        }

        Ok(hello)
    }

    /// Returns the documented heartbeat interval as a [`Duration`].
    #[must_use]
    pub fn interval(self) -> Duration {
        // [`Hello::parse`] rejects non-positive intervals, so the conversion is
        // infallible for a parsed `Hello`; the fallback keeps this helper
        // panic-free for a directly constructed one.
        Duration::from_millis(u64::try_from(self.heartbeat_interval).unwrap_or_default())
    }
}

/// The `properties` object of an Identify payload.
///
/// Reference: `docs/gateway-events.md` (Identify Connection Properties).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentifyProperties {
    /// The operating system the client runs on.
    pub os: String,
    /// The client/library name.
    pub browser: String,
    /// The client/library name.
    pub device: String,
}

impl Default for IdentifyProperties {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS.to_owned(),
            browser: env!("CARGO_PKG_NAME").to_owned(),
            device: env!("CARGO_PKG_NAME").to_owned(),
        }
    }
}

/// Builds an Identify (op 2) payload.
///
/// `compress` is always `false`: the client asks Discord not to compress
/// payloads. `shard` and the initial `presence` are only included when given.
///
/// Reference: `docs/gateway-events.md` (Identify), `docs/gateway.md`
/// (Identifying, Sharding).
#[must_use]
pub fn identify(
    token: &str,
    intents: GatewayIntents,
    shard: Option<(i64, i64)>,
    presence: Option<Value>,
) -> Value {
    let mut data = json!({
        "token": token,
        "intents": intents.bits(),
        "properties": IdentifyProperties::default(),
        "compress": false,
    });

    if let Some((shard_id, shard_count)) = shard {
        data["shard"] = json!([shard_id, shard_count]);
    }

    if let Some(presence) = presence {
        data["presence"] = presence;
    }

    json!({ "op": Opcode::Identify.as_i64(), "d": data })
}

/// Builds a Resume (op 6) payload.
///
/// Reference: `docs/gateway-events.md` (Resume), `docs/gateway.md` (Resuming).
#[must_use]
pub fn resume(token: &str, session_id: &str, seq: i64) -> Value {
    json!({
        "op": Opcode::Resume.as_i64(),
        "d": {
            "token": token,
            "session_id": session_id,
            "seq": seq,
        },
    })
}

/// Builds a Heartbeat (op 1) payload.
///
/// `seq` is the last sequence number received, or `None` (serialised as `null`)
/// when no dispatch event has been received yet.
///
/// Reference: `docs/gateway-events.md` (Heartbeat), `docs/gateway.md`
/// (Sending Heartbeats).
#[must_use]
pub fn heartbeat(seq: Option<i64>) -> Value {
    json!({ "op": Opcode::Heartbeat.as_i64(), "d": seq })
}

/// Builds an Update Presence (op 3) payload around a presence object.
///
/// The presence object itself is produced by the presence builder; this function
/// only wraps it in the gateway envelope.
///
/// Reference: `docs/gateway-events.md` (Update Presence).
#[must_use]
pub fn presence_update(presence: Value) -> Value {
    let mut payload = serde_json::Map::with_capacity(2);
    payload.insert(
        "op".to_owned(),
        Value::from(Opcode::PresenceUpdate.as_i64()),
    );
    payload.insert("d".to_owned(), presence);

    Value::Object(payload)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn identify_data() -> Value {
        identify(
            "my_token",
            GatewayIntents::GUILDS.union(GatewayIntents::GUILD_MODERATION),
            None,
            None,
        )
    }

    #[test]
    fn opcodes_match_the_documented_numbers() {
        assert_eq!(Opcode::Dispatch.as_i64(), 0);
        assert_eq!(Opcode::Heartbeat.as_i64(), 1);
        assert_eq!(Opcode::Identify.as_i64(), 2);
        assert_eq!(Opcode::PresenceUpdate.as_i64(), 3);
        assert_eq!(Opcode::Resume.as_i64(), 6);
        assert_eq!(Opcode::Reconnect.as_i64(), 7);
        assert_eq!(Opcode::InvalidSession.as_i64(), 9);
        assert_eq!(Opcode::Hello.as_i64(), 10);
        assert_eq!(Opcode::HeartbeatAck.as_i64(), 11);

        assert_eq!(Opcode::from_i64(0), Some(Opcode::Dispatch));
        assert_eq!(Opcode::from_i64(11), Some(Opcode::HeartbeatAck));
        assert_eq!(Opcode::from_i64(42), None);
    }

    #[test]
    fn identify_matches_the_documented_shape() {
        let payload = identify_data();

        assert_eq!(payload["op"], 2);
        assert_eq!(payload["d"]["token"], "my_token");
        assert_eq!(payload["d"]["intents"], 5);
        assert_eq!(payload["d"]["compress"], false);
        assert!(payload["d"]["properties"]["os"].is_string());
        assert_eq!(
            payload["d"]["properties"]["browser"],
            env!("CARGO_PKG_NAME")
        );
        assert_eq!(payload["d"]["properties"]["device"], env!("CARGO_PKG_NAME"));
        // Optional fields are omitted unless requested.
        assert!(payload["d"].get("shard").is_none());
        assert!(payload["d"].get("presence").is_none());
        assert!(payload.get("s").is_none());
        assert!(payload.get("t").is_none());
    }

    #[test]
    fn identify_can_carry_a_shard_and_initial_presence() {
        let presence = json!({"since": null, "activities": [], "status": "online", "afk": false});
        let payload = identify(
            "token",
            GatewayIntents::GUILDS,
            Some((1, 3)),
            Some(presence.clone()),
        );

        assert_eq!(payload["d"]["shard"], json!([1, 3]));
        assert_eq!(payload["d"]["presence"], presence);
    }

    #[test]
    fn resume_matches_the_documented_shape() {
        let payload = resume("randomstring", "evenmorerandomstring", 1337);

        assert_eq!(
            payload,
            json!({
                "op": 6,
                "d": {
                    "token": "randomstring",
                    "session_id": "evenmorerandomstring",
                    "seq": 1337,
                },
            })
        );
    }

    #[test]
    fn heartbeat_sends_the_last_sequence_number_or_null() {
        assert_eq!(heartbeat(Some(251)), json!({"op": 1, "d": 251}));
        assert_eq!(heartbeat(None), json!({"op": 1, "d": null}));
    }

    #[test]
    fn presence_update_wraps_the_presence_object() {
        let presence = json!({"since": null, "activities": [], "status": "online", "afk": false});
        let payload = presence_update(presence.clone());

        assert_eq!(payload, json!({"op": 3, "d": presence}));
    }

    #[test]
    fn hello_is_parsed_and_rejects_a_non_positive_interval() {
        let hello = decode(r#"{"op":10,"d":{"heartbeat_interval":45000}}"#).expect("decodes");
        let hello = Hello::parse(&hello).expect("valid hello");

        assert_eq!(hello.interval(), Duration::from_secs(45));

        let invalid = decode(r#"{"op":10,"d":{"heartbeat_interval":0}}"#).expect("decodes");
        assert!(matches!(
            Hello::parse(&invalid),
            Err(Error::Gateway(GatewayError::Protocol(_)))
        ));
    }

    #[test]
    fn dispatch_payload_keeps_sequence_number_and_event_name() {
        let payload =
            decode(r#"{"op":0,"s":42,"t":"MESSAGE_CREATE","d":{"id":"1"}}"#).expect("decodes");

        assert_eq!(payload.op, 0);
        assert_eq!(payload.s, Some(42));
        assert_eq!(payload.t.as_deref(), Some("MESSAGE_CREATE"));
        assert_eq!(payload.d["id"], "1");
    }

    #[test]
    fn non_dispatch_payloads_default_the_optional_fields() {
        let payload = decode(r#"{"op":11}"#).expect("decodes");

        assert_eq!(payload.op, 11);
        assert_eq!(payload.d, Value::Null);
        assert_eq!(payload.s, None);
        assert_eq!(payload.t, None);
    }
}
