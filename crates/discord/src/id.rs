//! Discord snowflake identifiers.
//!
//! Discord identifies every object with a 64-bit integer it calls a
//! *snowflake*. On the wire they are serialised as JSON strings, which is why
//! [`Id`] has its own `Serialize`/`Deserialize` implementations instead of
//! being a bare `u64`.
//!
//! The typed marker argument prevents passing a channel id where a user id is
//! expected. The marker types are uninhabited enums, so they cost nothing at
//! runtime.
//!
//! Reference: `docs/reference.md` (Snowflakes, ID Serialization).

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;

use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::timestamp::Timestamp;

/// Milliseconds between the Unix epoch and the Discord epoch (2015-01-01).
///
/// Reference: `docs/reference.md` (Snowflakes - Convert Snowflake to `DateTime`).
pub(crate) const DISCORD_EPOCH_MILLIS: u64 = 1_420_070_400_000;

/// A snowflake carrying a marker type for the kind of object it identifies.
///
/// `Id` is [`Copy`], comparable, hashable and serialises as a decimal string,
/// matching Discord's wire format.
pub struct Id<Kind> {
    value: u64,
    kind: PhantomData<fn() -> Kind>,
}

impl<Kind> Id<Kind> {
    /// Creates an id from a raw integer.
    ///
    /// # Panics
    ///
    /// Panics when `value` is zero. Discord never emits a zero snowflake, and
    /// accepting one silently would turn a missing value into a plausible id.
    /// Configuration code that parses user-supplied ids must reject zero before
    /// calling this (see `src/config.rs`).
    #[must_use]
    pub fn new(value: u64) -> Self {
        assert!(value != 0, "a Discord snowflake must not be zero");

        Self {
            value,
            kind: PhantomData,
        }
    }

    /// Returns the raw integer value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.value
    }

    /// Returns the time at which the object identified by this snowflake was
    /// created, derived from its high bits.
    #[must_use]
    pub const fn created_at(self) -> Timestamp {
        // `value >> 22` is at most 2^42 - 1 and the Discord epoch is about
        // 2^40 milliseconds, so their sum always fits an `i64`. `cast_signed`
        // spells out that sign conversion; `i64::try_from` is not usable here
        // because this stays a `const fn`.
        Timestamp::from_unix_millis(((self.value >> 22) + DISCORD_EPOCH_MILLIS).cast_signed())
    }
}

impl<Kind> Clone for Id<Kind> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Kind> Copy for Id<Kind> {}

impl<Kind> PartialEq for Id<Kind> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<Kind> Eq for Id<Kind> {}

impl<Kind> PartialOrd for Id<Kind> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<Kind> Ord for Id<Kind> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<Kind> Hash for Id<Kind> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<Kind> fmt::Display for Id<Kind> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.value, f)
    }
}

impl<Kind> fmt::Debug for Id<Kind> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.value)
    }
}

impl<Kind> FromStr for Id<Kind> {
    type Err = std::num::ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let value = text.parse()?;

        Ok(Self::new(value))
    }
}

impl<Kind> Serialize for Id<Kind> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.value.to_string())
    }
}

impl<'de, Kind> Deserialize<'de> for Id<Kind> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        /// Discord sends snowflakes as strings; a number is tolerated because
        /// the reference notes that ids not originating from Discord can be
        /// serialised as numbers.
        struct IdVisitor;

        impl Visitor<'_> for IdVisitor {
            type Value = u64;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a Discord snowflake as a string or an integer")
            }

            fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
                value.parse().map_err(E::custom)
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                Ok(value)
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                u64::try_from(value).map_err(|_| E::custom("a snowflake must be positive"))
            }
        }

        let value = deserializer.deserialize_any(IdVisitor)?;

        if value == 0 {
            return Err(D::Error::custom("a Discord snowflake must not be zero"));
        }

        Ok(Self::new(value))
    }
}

// Marker types for the resource kinds this crate models.
//
// They are uninhabited: only their type identity matters, so they cost nothing
// at runtime. Each one names the id alias it belongs to.
/// Marker type for [`ApplicationId`].
#[derive(Debug)]
pub enum ApplicationMarker {}
/// Marker type for [`AttachmentId`].
#[derive(Debug)]
pub enum AttachmentMarker {}
/// Marker type for [`AuditLogEntryId`].
#[derive(Debug)]
pub enum AuditLogEntryMarker {}
/// Marker type for [`ChannelId`].
#[derive(Debug)]
pub enum ChannelMarker {}
/// Marker type for [`CommandId`].
#[derive(Debug)]
pub enum CommandMarker {}
/// Marker type for [`EmojiId`].
#[derive(Debug)]
pub enum EmojiMarker {}
/// Marker type for [`GenericId`].
#[derive(Debug)]
pub enum GenericMarker {}
/// Marker type for [`GuildId`].
#[derive(Debug)]
pub enum GuildMarker {}
/// Marker type for [`InteractionId`].
#[derive(Debug)]
pub enum InteractionMarker {}
/// Marker type for [`MessageId`].
#[derive(Debug)]
pub enum MessageMarker {}
/// Marker type for [`RoleId`].
#[derive(Debug)]
pub enum RoleMarker {}
/// Marker type for [`UserId`].
#[derive(Debug)]
pub enum UserMarker {}
/// Marker type for [`WebhookId`].
#[derive(Debug)]
pub enum WebhookMarker {}

/// An application (bot) id.
pub type ApplicationId = Id<ApplicationMarker>;
/// An attachment id.
pub type AttachmentId = Id<AttachmentMarker>;
/// An audit log entry id.
pub type AuditLogEntryId = Id<AuditLogEntryMarker>;
/// A channel id.
pub type ChannelId = Id<ChannelMarker>;
/// An application command id.
pub type CommandId = Id<CommandMarker>;
/// An emoji id.
pub type EmojiId = Id<EmojiMarker>;
/// A snowflake whose kind is not modelled by a dedicated marker.
///
/// The audit log calls these "the id of the affected entity, which may be a
/// webhook, user, role, ..."; the kind is only knowable from the action type.
pub type GenericId = Id<GenericMarker>;
/// A guild (server) id.
pub type GuildId = Id<GuildMarker>;
/// An interaction id.
pub type InteractionId = Id<InteractionMarker>;
/// A message id.
pub type MessageId = Id<MessageMarker>;
/// A role id.
pub type RoleId = Id<RoleMarker>;
/// A user id.
pub type UserId = Id<UserMarker>;
/// A webhook id.
pub type WebhookId = Id<WebhookMarker>;

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn display_and_serde_round_trip_the_decimal_string() {
        let id = ChannelId::new(1_111_111_111_111_111_111);

        assert_eq!(id.to_string(), "1111111111111111111");

        let json = serde_json::to_string(&id).expect("serialisable");
        assert_eq!(json, "\"1111111111111111111\"");

        let parsed: ChannelId = serde_json::from_str(&json).expect("deserialisable");
        assert_eq!(parsed, id);
    }

    #[test]
    fn serde_accepts_a_bare_integer_for_non_discord_payloads() {
        let parsed: UserId = serde_json::from_str("42").expect("integer accepted");

        assert_eq!(parsed, UserId::new(42));
    }

    #[test]
    fn zero_is_rejected_on_the_wire() {
        let err = serde_json::from_str::<UserId>("\"0\"").expect_err("zero rejected");

        assert!(err.to_string().contains("zero"));
    }

    #[test]
    fn ids_compare_and_hash_by_value() {
        let mut map = HashMap::new();
        map.insert(GuildId::new(1), "a");
        map.insert(GuildId::new(2), "b");

        assert_eq!(map.get(&GuildId::new(1)), Some(&"a"));
        assert!(GuildId::new(1) < GuildId::new(2));
    }

    #[test]
    fn created_at_uses_the_documented_epoch() {
        // The high bits of a snowflake are milliseconds since the Discord epoch,
        // so a timestamp part of 1000 is one second after 2015-01-01T00:00:00Z.
        let id = MessageId::new(1_000u64 << 22);

        assert_eq!(id.created_at().to_string(), "2015-01-01T00:00:01.000+00:00");
    }

    #[test]
    fn created_at_inverts_the_documented_snowflake_formula() {
        // docs/reference.md (Snowflakes): the timestamp field is
        // `(snowflake >> 22) + 1420070400000` milliseconds.
        const DISCORD_EPOCH: i64 = 1_420_070_400_000;
        // The lower 22 bits are the worker/process/increment fields; setting them
        // to their documented maximums must not influence the timestamp.
        const LOW_BITS: u64 = (0b1_1111 << 17) | (0b1_1111 << 12) | 0xFFF;

        for offset_millis in [0, 1, 86_400_000, 1_437_152_400_000 - DISCORD_EPOCH] {
            let timestamp_millis = DISCORD_EPOCH + offset_millis;
            let high = u64::try_from(offset_millis).expect("a non-negative offset") << 22;
            let id = GenericId::new(high | LOW_BITS);

            assert_eq!(
                id.created_at(),
                Timestamp::from_unix_millis(timestamp_millis),
                "offset {offset_millis}ms"
            );
        }
    }

    #[test]
    fn a_snowflake_with_only_increment_bits_is_at_the_discord_epoch() {
        let id = GenericId::new(0xFFF);

        assert_eq!(
            id.created_at().unix_millis(),
            i64::try_from(DISCORD_EPOCH_MILLIS).expect("the epoch fits in an i64")
        );
    }

    #[test]
    fn deserialization_rejects_values_that_are_not_snowflakes() {
        // docs/reference.md (Snowflakes): ids are unsigned 64-bit integers,
        // returned as decimal strings.
        assert!(serde_json::from_str::<UserId>("-3").is_err());
        assert!(serde_json::from_str::<UserId>("1.5").is_err());
        assert!(serde_json::from_str::<UserId>("true").is_err());
        assert!(serde_json::from_str::<UserId>("\"\"").is_err());
    }

    #[test]
    fn from_str_parses_the_documented_string_form() {
        let id: GuildId = "41771983423143937".parse().expect("a valid snowflake");

        assert_eq!(id, GuildId::new(41_771_983_423_143_937));
    }

    #[test]
    fn serializes_large_ids_without_losing_precision() {
        let id = MessageId::new(1_522_667_998_653_841_418);

        assert_eq!(
            serde_json::to_string(&id).expect("serialisable"),
            "\"1522667998653841418\""
        );
    }

    #[test]
    #[should_panic(expected = "must not be zero")]
    fn new_rejects_zero() {
        let _ = UserId::new(0);
    }
}
