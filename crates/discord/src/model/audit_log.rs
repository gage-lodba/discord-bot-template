//! Audit log entry models.
//!
//! Reference: `docs/audit-log.md` (Audit Log Entry Object, Audit Log Events,
//! Audit Log Change Object) and `docs/gateway-events.md`
//! (Guild Audit Log Entry Create).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::{de::Error as DeError, de::Visitor};
use std::fmt;

use crate::id::{AuditLogEntryId, GenericId, GuildId, UserId};

/// Generates [`AuditLogAction`] from the `action_type` table in
/// `docs/audit-log.md` (Audit Log Events).
macro_rules! audit_log_actions {
    ($( $variant:ident = $value:literal, )*) => {
        /// The `action_type` of an audit log entry.
        ///
        /// Every event in `docs/audit-log.md` (Audit Log Events) has a variant;
        /// an event this crate does not model deserialises to
        /// [`AuditLogAction::Other`] so a new Discord action can never break a
        /// handler.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum AuditLogAction {
            $(
                #[doc = concat!(
                    "`", stringify!($variant), "` (`action_type` ", stringify!($value), ")."
                )]
                $variant,
            )*
            /// An action type this crate does not model, kept as sent.
            Other(i64),
        }

        impl AuditLogAction {
            /// Maps the integer `action_type` value to an [`AuditLogAction`].
            ///
            /// Unknown values become [`AuditLogAction::Other`].
            #[must_use]
            pub const fn from_i64(value: i64) -> Self {
                match value {
                    $( $value => Self::$variant, )*
                    other => Self::Other(other),
                }
            }

            /// Returns the integer `action_type` value.
            #[must_use]
            pub const fn as_i64(self) -> i64 {
                match self {
                    $( Self::$variant => $value, )*
                    Self::Other(value) => value,
                }
            }
        }
    };
}

audit_log_actions! {
    GuildUpdate = 1,
    ChannelCreate = 10,
    ChannelUpdate = 11,
    ChannelDelete = 12,
    ChannelOverwriteCreate = 13,
    ChannelOverwriteUpdate = 14,
    ChannelOverwriteDelete = 15,
    MemberKick = 20,
    MemberPrune = 21,
    MemberBanAdd = 22,
    MemberBanRemove = 23,
    MemberUpdate = 24,
    MemberRoleUpdate = 25,
    MemberMove = 26,
    MemberDisconnect = 27,
    BotAdd = 28,
    RoleCreate = 30,
    RoleUpdate = 31,
    RoleDelete = 32,
    InviteCreate = 40,
    InviteUpdate = 41,
    InviteDelete = 42,
    WebhookCreate = 50,
    WebhookUpdate = 51,
    WebhookDelete = 52,
    EmojiCreate = 60,
    EmojiUpdate = 61,
    EmojiDelete = 62,
    MessageDelete = 72,
    MessageBulkDelete = 73,
    MessagePin = 74,
    MessageUnpin = 75,
    IntegrationCreate = 80,
    IntegrationUpdate = 81,
    IntegrationDelete = 82,
    StageInstanceCreate = 83,
    StageInstanceUpdate = 84,
    StageInstanceDelete = 85,
    StickerCreate = 90,
    StickerUpdate = 91,
    StickerDelete = 92,
    GuildScheduledEventCreate = 100,
    GuildScheduledEventUpdate = 101,
    GuildScheduledEventDelete = 102,
    ThreadCreate = 110,
    ThreadUpdate = 111,
    ThreadDelete = 112,
    ApplicationCommandPermissionUpdate = 121,
    SoundboardSoundCreate = 130,
    SoundboardSoundUpdate = 131,
    SoundboardSoundDelete = 132,
    AutoModerationRuleCreate = 140,
    AutoModerationRuleUpdate = 141,
    AutoModerationRuleDelete = 142,
    AutoModerationBlockMessage = 143,
    AutoModerationFlagToChannel = 144,
    AutoModerationUserCommunicationDisabled = 145,
    AutoModerationQuarantineUser = 146,
    CreatorMonetizationRequestCreated = 150,
    CreatorMonetizationTermsAccepted = 151,
    OnboardingPromptCreate = 163,
    OnboardingPromptUpdate = 164,
    OnboardingPromptDelete = 165,
    OnboardingCreate = 166,
    OnboardingUpdate = 167,
    HomeSettingsCreate = 190,
    HomeSettingsUpdate = 191,
    VoiceChannelStatusCreate = 192,
    VoiceChannelStatusDelete = 193,
}

impl Serialize for AuditLogAction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for AuditLogAction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ActionVisitor;

        impl Visitor<'_> for ActionVisitor {
            type Value = AuditLogAction;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an audit log action_type integer")
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                Ok(AuditLogAction::from_i64(value))
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                i64::try_from(value)
                    .map(AuditLogAction::from_i64)
                    .map_err(|_| E::custom("audit log action_type out of range"))
            }
        }

        deserializer.deserialize_i64(ActionVisitor)
    }
}

/// One entry in a guild's audit log.
///
/// The `guild_id` field is an extra field of the `GUILD_AUDIT_LOG_ENTRY_CREATE`
/// gateway event; it is absent when the entry comes from the REST audit log
/// endpoint.
///
/// Reference: `docs/audit-log.md` (Audit Log Entry Structure) and
/// `docs/gateway-events.md` (Guild Audit Log Entry Create).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// The id of the entry.
    pub id: AuditLogEntryId,
    /// The type of action that occurred, from the `action_type` field.
    #[serde(rename = "action_type")]
    pub action: AuditLogAction,
    /// The user or app that made the change.
    #[serde(default)]
    pub user_id: Option<UserId>,
    /// The id of the affected entity (webhook, user, role, ...).
    #[serde(default)]
    pub target_id: Option<GenericId>,
    /// The reason for the change, from the `X-Audit-Log-Reason` header.
    #[serde(default)]
    pub reason: Option<String>,
    /// The changes made to [`AuditLogEntry::target_id`].
    #[serde(default)]
    pub changes: Vec<AuditLogChange>,
    /// Additional information for certain event types
    /// (`docs/audit-log.md`, Optional Audit Entry Info).
    #[serde(default)]
    pub options: Option<serde_json::Value>,
    /// The guild id, present on the gateway event.
    #[serde(default)]
    pub guild_id: Option<GuildId>,
}

/// One changed property inside an [`AuditLogEntry`].
///
/// The type of `new_value`/`old_value` depends on the changed object and the
/// change key, so both stay as raw JSON.
///
/// Reference: `docs/audit-log.md` (Audit Log Change Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditLogChange {
    /// The name of the changed entity.
    pub key: String,
    /// The new value of the key, when present.
    #[serde(default)]
    pub new_value: Option<serde_json::Value>,
    /// The old value of the key, when present.
    #[serde(default)]
    pub old_value: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_types_cover_the_documented_table() {
        for (value, action) in [
            (1, AuditLogAction::GuildUpdate),
            (10, AuditLogAction::ChannelCreate),
            (11, AuditLogAction::ChannelUpdate),
            (12, AuditLogAction::ChannelDelete),
            (13, AuditLogAction::ChannelOverwriteCreate),
            (14, AuditLogAction::ChannelOverwriteUpdate),
            (15, AuditLogAction::ChannelOverwriteDelete),
            (20, AuditLogAction::MemberKick),
            (21, AuditLogAction::MemberPrune),
            (22, AuditLogAction::MemberBanAdd),
            (23, AuditLogAction::MemberBanRemove),
            (24, AuditLogAction::MemberUpdate),
            (25, AuditLogAction::MemberRoleUpdate),
            (26, AuditLogAction::MemberMove),
            (27, AuditLogAction::MemberDisconnect),
            (28, AuditLogAction::BotAdd),
            (30, AuditLogAction::RoleCreate),
            (31, AuditLogAction::RoleUpdate),
            (32, AuditLogAction::RoleDelete),
            (40, AuditLogAction::InviteCreate),
            (41, AuditLogAction::InviteUpdate),
            (42, AuditLogAction::InviteDelete),
            (50, AuditLogAction::WebhookCreate),
            (51, AuditLogAction::WebhookUpdate),
            (52, AuditLogAction::WebhookDelete),
            (60, AuditLogAction::EmojiCreate),
            (61, AuditLogAction::EmojiUpdate),
            (62, AuditLogAction::EmojiDelete),
            (72, AuditLogAction::MessageDelete),
            (73, AuditLogAction::MessageBulkDelete),
            (74, AuditLogAction::MessagePin),
            (75, AuditLogAction::MessageUnpin),
            (80, AuditLogAction::IntegrationCreate),
            (81, AuditLogAction::IntegrationUpdate),
            (82, AuditLogAction::IntegrationDelete),
            (83, AuditLogAction::StageInstanceCreate),
            (84, AuditLogAction::StageInstanceUpdate),
            (85, AuditLogAction::StageInstanceDelete),
            (90, AuditLogAction::StickerCreate),
            (91, AuditLogAction::StickerUpdate),
            (92, AuditLogAction::StickerDelete),
            (100, AuditLogAction::GuildScheduledEventCreate),
            (101, AuditLogAction::GuildScheduledEventUpdate),
            (102, AuditLogAction::GuildScheduledEventDelete),
            (110, AuditLogAction::ThreadCreate),
            (111, AuditLogAction::ThreadUpdate),
            (112, AuditLogAction::ThreadDelete),
            (121, AuditLogAction::ApplicationCommandPermissionUpdate),
            (130, AuditLogAction::SoundboardSoundCreate),
            (131, AuditLogAction::SoundboardSoundUpdate),
            (132, AuditLogAction::SoundboardSoundDelete),
            (140, AuditLogAction::AutoModerationRuleCreate),
            (141, AuditLogAction::AutoModerationRuleUpdate),
            (142, AuditLogAction::AutoModerationRuleDelete),
            (143, AuditLogAction::AutoModerationBlockMessage),
            (144, AuditLogAction::AutoModerationFlagToChannel),
            (145, AuditLogAction::AutoModerationUserCommunicationDisabled),
            (146, AuditLogAction::AutoModerationQuarantineUser),
            (150, AuditLogAction::CreatorMonetizationRequestCreated),
            (151, AuditLogAction::CreatorMonetizationTermsAccepted),
            (163, AuditLogAction::OnboardingPromptCreate),
            (164, AuditLogAction::OnboardingPromptUpdate),
            (165, AuditLogAction::OnboardingPromptDelete),
            (166, AuditLogAction::OnboardingCreate),
            (167, AuditLogAction::OnboardingUpdate),
            (190, AuditLogAction::HomeSettingsCreate),
            (191, AuditLogAction::HomeSettingsUpdate),
            (192, AuditLogAction::VoiceChannelStatusCreate),
            (193, AuditLogAction::VoiceChannelStatusDelete),
        ] {
            assert_eq!(AuditLogAction::from_i64(value), action);
            assert_eq!(action.as_i64(), value);

            let json = serde_json::to_value(action).expect("serialisable");
            assert_eq!(json, serde_json::json!(value));

            let parsed: AuditLogAction = serde_json::from_value(json).expect("deserialisable");
            assert_eq!(parsed, action);
        }
    }

    #[test]
    fn unknown_action_types_are_preserved() {
        let action = AuditLogAction::from_i64(9999);

        assert_eq!(action, AuditLogAction::Other(9999));
        assert_eq!(action.as_i64(), 9999);

        let parsed: AuditLogAction =
            serde_json::from_value(serde_json::json!(9999)).expect("parsed");
        assert_eq!(parsed, AuditLogAction::Other(9999));
    }

    #[test]
    fn parses_a_member_ban_audit_log_entry() {
        let entry: AuditLogEntry = serde_json::from_value(serde_json::json!({
            "id": "610036311282647040",
            "action_type": 22,
            "user_id": "53908099506183680",
            "target_id": "245538417976458753",
            "reason": "mentioning b1nzy",
            "changes": [],
            "options": {
                "channel_id": "290926798626357248"
            },
            "guild_id": "290926798626357250"
        }))
        .expect("documented audit log entry shape");

        assert_eq!(entry.action, AuditLogAction::MemberBanAdd);
        assert_eq!(entry.user_id, Some(UserId::new(53_908_099_506_183_680)));
        assert_eq!(
            entry.target_id,
            Some(GenericId::new(245_538_417_976_458_753))
        );
        assert_eq!(entry.reason.as_deref(), Some("mentioning b1nzy"));
        assert_eq!(entry.guild_id, Some(GuildId::new(290_926_798_626_357_250)));
        assert_eq!(
            entry.options,
            Some(serde_json::json!({"channel_id": "290926798626357248"}))
        );
    }

    #[test]
    fn parses_audit_log_changes_with_new_and_old_values() {
        let entry: AuditLogEntry = serde_json::from_value(serde_json::json!({
            "id": "1",
            "action_type": 24,
            "changes": [
                {"key": "nick", "new_value": "new nick", "old_value": "old nick"},
                {"key": "deaf", "new_value": true},
                {"key": "mute", "old_value": false},
                {"key": "$add", "new_value": [{"id": "1", "name": "role"}]}
            ]
        }))
        .expect("entry with changes");

        assert_eq!(entry.action, AuditLogAction::MemberUpdate);
        assert_eq!(entry.changes.len(), 4);
        assert_eq!(entry.changes[0].key, "nick");
        assert_eq!(
            entry.changes[0].new_value,
            Some(serde_json::json!("new nick"))
        );
        assert_eq!(
            entry.changes[0].old_value,
            Some(serde_json::json!("old nick"))
        );
        assert_eq!(entry.changes[1].old_value, None);
        assert_eq!(entry.changes[2].new_value, None);
        assert_eq!(
            entry.changes[3].new_value,
            Some(serde_json::json!([{"id": "1", "name": "role"}]))
        );
    }

    #[test]
    fn changes_and_optional_fields_default_when_absent() {
        let entry: AuditLogEntry = serde_json::from_value(serde_json::json!({
            "id": "1",
            "action_type": 20
        }))
        .expect("minimal entry");

        assert_eq!(entry.action, AuditLogAction::MemberKick);
        assert_eq!(entry.user_id, None);
        assert_eq!(entry.target_id, None);
        assert_eq!(entry.reason, None);
        assert!(entry.changes.is_empty());
        assert_eq!(entry.options, None);
        assert_eq!(entry.guild_id, None);
    }

    #[test]
    fn serialising_an_entry_uses_action_type_on_the_wire() {
        let entry: AuditLogEntry = serde_json::from_value(serde_json::json!({
            "id": "1",
            "action_type": 23
        }))
        .expect("entry");

        let json = serde_json::to_value(&entry).expect("serialisable");

        assert_eq!(json["action_type"], serde_json::json!(23));
        assert_eq!(json["id"], serde_json::json!("1"));
    }
}
