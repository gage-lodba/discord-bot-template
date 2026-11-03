//! Gateway intents.
//!
//! Intents are a bitfield sent with the Identify payload that tells Discord
//! which event families the client wants. Requesting an event family that is
//! not enabled for the application closes the connection with code 4014, so the
//! constants here are the exact documented values.
//!
//! Reference: `docs/gateway.md` (Gateway Intents - List of Intents) and
//! `docs/gateway-events.md` (Identify).

use std::fmt;

use serde::{Deserialize, Serialize};

/// A bitfield of gateway intents.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GatewayIntents(u64);

impl GatewayIntents {
    /// Returns an empty intent set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns the raw bits.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Returns the union of two intent sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Returns true when every intent in `required` is set.
    #[must_use]
    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    /// Events for guild create/update/delete, channels, threads and roles.
    pub const GUILDS: Self = Self(1 << 0);
    /// Guild member add/update/remove events (privileged).
    pub const GUILD_MEMBERS: Self = Self(1 << 1);
    /// Audit log entry create and ban add/remove events.
    pub const GUILD_MODERATION: Self = Self(1 << 2);
    /// Emoji, sticker and soundboard update events.
    pub const GUILD_EXPRESSIONS: Self = Self(1 << 3);
    /// Guild integration events.
    pub const GUILD_INTEGRATIONS: Self = Self(1 << 4);
    /// Webhook update events.
    pub const GUILD_WEBHOOKS: Self = Self(1 << 5);
    /// Invite create/delete events.
    pub const GUILD_INVITES: Self = Self(1 << 6);
    /// Voice state update events.
    pub const GUILD_VOICE_STATES: Self = Self(1 << 7);
    /// Presence update events (privileged).
    pub const GUILD_PRESENCES: Self = Self(1 << 8);
    /// Guild message create/update/delete events.
    pub const GUILD_MESSAGES: Self = Self(1 << 9);
    /// Guild message reaction events.
    pub const GUILD_MESSAGE_REACTIONS: Self = Self(1 << 10);
    /// Guild typing start events.
    pub const GUILD_MESSAGE_TYPING: Self = Self(1 << 11);
    /// Direct message create/update/delete events.
    pub const DIRECT_MESSAGES: Self = Self(1 << 12);
    /// Direct message reaction events.
    pub const DIRECT_MESSAGE_REACTIONS: Self = Self(1 << 13);
    /// Direct message typing start events.
    pub const DIRECT_MESSAGE_TYPING: Self = Self(1 << 14);
    /// Adds content fields to message events (privileged).
    pub const MESSAGE_CONTENT: Self = Self(1 << 15);
    /// Guild scheduled event events.
    pub const GUILD_SCHEDULED_EVENTS: Self = Self(1 << 16);
    /// Auto moderation rule create/update/delete events.
    pub const AUTO_MODERATION_CONFIGURATION: Self = Self(1 << 20);
    /// Auto moderation action execution events.
    pub const AUTO_MODERATION_EXECUTION: Self = Self(1 << 21);
    /// Guild message poll vote events.
    pub const GUILD_MESSAGE_POLLS: Self = Self(1 << 24);
    /// Direct message poll vote events.
    pub const DIRECT_MESSAGE_POLLS: Self = Self(1 << 25);
}

impl fmt::Debug for GatewayIntents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GatewayIntents(0b{:b})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_values_match_the_reference() {
        assert_eq!(GatewayIntents::GUILDS.bits(), 1);
        assert_eq!(GatewayIntents::GUILD_MEMBERS.bits(), 2);
        assert_eq!(GatewayIntents::GUILD_MODERATION.bits(), 4);
        assert_eq!(GatewayIntents::GUILD_MESSAGES.bits(), 512);
        assert_eq!(GatewayIntents::MESSAGE_CONTENT.bits(), 32_768);
    }

    #[test]
    fn union_and_contains_work() {
        let intents = GatewayIntents::GUILDS.union(GatewayIntents::GUILD_MODERATION);

        assert_eq!(intents.bits(), 5);
        assert!(intents.contains(GatewayIntents::GUILDS));
        assert!(!intents.contains(GatewayIntents::GUILD_MEMBERS));
    }

    #[test]
    fn every_documented_intent_has_the_documented_bit() {
        // docs/gateway.md (Gateway Intents — List of Intents). The documented
        // list ends at DIRECT_MESSAGE_POLLS (1 << 25); bits 17-19 and 22-23 are
        // not assigned to any intent.
        let documented = [
            ("GUILDS", 0, GatewayIntents::GUILDS),
            ("GUILD_MEMBERS", 1, GatewayIntents::GUILD_MEMBERS),
            ("GUILD_MODERATION", 2, GatewayIntents::GUILD_MODERATION),
            ("GUILD_EXPRESSIONS", 3, GatewayIntents::GUILD_EXPRESSIONS),
            ("GUILD_INTEGRATIONS", 4, GatewayIntents::GUILD_INTEGRATIONS),
            ("GUILD_WEBHOOKS", 5, GatewayIntents::GUILD_WEBHOOKS),
            ("GUILD_INVITES", 6, GatewayIntents::GUILD_INVITES),
            ("GUILD_VOICE_STATES", 7, GatewayIntents::GUILD_VOICE_STATES),
            ("GUILD_PRESENCES", 8, GatewayIntents::GUILD_PRESENCES),
            ("GUILD_MESSAGES", 9, GatewayIntents::GUILD_MESSAGES),
            (
                "GUILD_MESSAGE_REACTIONS",
                10,
                GatewayIntents::GUILD_MESSAGE_REACTIONS,
            ),
            (
                "GUILD_MESSAGE_TYPING",
                11,
                GatewayIntents::GUILD_MESSAGE_TYPING,
            ),
            ("DIRECT_MESSAGES", 12, GatewayIntents::DIRECT_MESSAGES),
            (
                "DIRECT_MESSAGE_REACTIONS",
                13,
                GatewayIntents::DIRECT_MESSAGE_REACTIONS,
            ),
            (
                "DIRECT_MESSAGE_TYPING",
                14,
                GatewayIntents::DIRECT_MESSAGE_TYPING,
            ),
            ("MESSAGE_CONTENT", 15, GatewayIntents::MESSAGE_CONTENT),
            (
                "GUILD_SCHEDULED_EVENTS",
                16,
                GatewayIntents::GUILD_SCHEDULED_EVENTS,
            ),
            (
                "AUTO_MODERATION_CONFIGURATION",
                20,
                GatewayIntents::AUTO_MODERATION_CONFIGURATION,
            ),
            (
                "AUTO_MODERATION_EXECUTION",
                21,
                GatewayIntents::AUTO_MODERATION_EXECUTION,
            ),
            (
                "GUILD_MESSAGE_POLLS",
                24,
                GatewayIntents::GUILD_MESSAGE_POLLS,
            ),
            (
                "DIRECT_MESSAGE_POLLS",
                25,
                GatewayIntents::DIRECT_MESSAGE_POLLS,
            ),
        ];

        let mut union = GatewayIntents::empty();

        for (name, position, intent) in documented {
            let bit = 1u64 << position;
            assert_eq!(intent.bits(), bit, "{name} is not 1 << {position}");
            union = union.union(intent);
        }

        assert_eq!(
            union.bits().count_ones(),
            u32::try_from(documented.len()).expect("the table fits in a u32"),
            "two documented intents share a bit"
        );
    }

    #[test]
    fn serializes_as_a_number_for_identify() {
        let json =
            serde_json::to_string(&GatewayIntents::GUILDS.union(GatewayIntents::GUILD_MESSAGES))
                .expect("serialisable");

        assert_eq!(json, "513");
    }
}
