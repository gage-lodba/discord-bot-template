//! Interaction and application command models.
//!
//! The interaction object and its data structures are documented in
//! `docs/receiving-and-responding.md` (Interaction Object, Interaction Data,
//! Resolved Data Structure, Interaction Response Object). This module models
//! all five documented interaction types:
//!
//! | Type | Model |
//! | - | - |
//! | `PING` (`1`) | [`PingInteraction`] |
//! | `APPLICATION_COMMAND` (`2`) | [`CommandInteraction`] |
//! | `MESSAGE_COMPONENT` (`3`) | [`ComponentInteraction`] |
//! | `APPLICATION_COMMAND_AUTOCOMPLETE` (`4`) | [`AutocompleteInteraction`] |
//! | `MODAL_SUBMIT` (`5`) | [`ModalSubmitInteraction`] |
//!
//! [`Interaction`] routes a payload by type. Deserialisation is lenient: an
//! interaction type this crate does not model stays [`Interaction::Unknown`],
//! and an unknown interaction context or callback type is preserved instead of
//! failing the payload.
//!
//! Interaction tokens are credentials: every `Debug` implementation in this
//! module redacts the `token` field (`docs/receiving-and-responding.md`,
//! Interaction Object).
//!
//! Resolved data is keyed by id and follows `docs/receiving-and-responding.md`
//! (Resolved Data Structure): partial members are missing `user`, `deaf` and
//! `mute` and are modelled as [`PartialMember`], and partial channels use
//! [`crate::model::channel::Channel`].
//!
//! Reference: `docs/receiving-and-responding.md`,
//! `docs/application-commands.md` (Autocomplete) and
//! `docs/gateway-events.md` (Interaction Create).

use std::collections::HashMap;
use std::fmt;

use serde::de::DeserializeOwned;
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::id::{
    ApplicationId, AttachmentId, ChannelId, CommandId, GenericId, GuildId, Id, InteractionId,
    MessageId, RoleId, UserId,
};
use crate::model::channel::Channel;
use crate::model::component::Component;
use crate::model::guild::{Member, Role};
use crate::model::message::{Attachment, Message};
use crate::model::user::User;
use crate::permissions::Permissions;

/// The documented interaction type: the integer `type` field.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionType {
    /// Type `1`: a ping interaction, an endpoint verification.
    Ping,
    /// Type `2`: an application command invocation.
    ApplicationCommand,
    /// Type `3`: a message component invocation.
    MessageComponent,
    /// Type `4`: an application command autocomplete interaction.
    ApplicationCommandAutocomplete,
    /// Type `5`: a modal submit interaction.
    ModalSubmit,
    /// An interaction type Discord added later, preserved as sent.
    Unknown(i64),
}

impl InteractionType {
    /// Maps the documented integer `type` value to an [`InteractionType`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::Ping,
            2 => Self::ApplicationCommand,
            3 => Self::MessageComponent,
            4 => Self::ApplicationCommandAutocomplete,
            5 => Self::ModalSubmit,
            other => Self::Unknown(other),
        }
    }

    /// Returns the documented integer `type` value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Ping => 1,
            Self::ApplicationCommand => 2,
            Self::MessageComponent => 3,
            Self::ApplicationCommandAutocomplete => 4,
            Self::ModalSubmit => 5,
            Self::Unknown(value) => value,
        }
    }
}

impl Serialize for InteractionType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for InteractionType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;

        Ok(Self::from_i64(value))
    }
}

/// The context an interaction was triggered from.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Context Types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionContext {
    /// Context `0`: within servers.
    Guild,
    /// Context `1`: within DMs with the app's bot user.
    BotDm,
    /// Context `2`: within group DMs and DMs other than the app's bot user.
    PrivateChannel,
    /// A context Discord added later, preserved as sent.
    Unknown(i64),
}

impl InteractionContext {
    /// Maps the documented integer context to an [`InteractionContext`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            0 => Self::Guild,
            1 => Self::BotDm,
            2 => Self::PrivateChannel,
            other => Self::Unknown(other),
        }
    }

    /// Returns the documented integer context.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Guild => 0,
            Self::BotDm => 1,
            Self::PrivateChannel => 2,
            Self::Unknown(value) => value,
        }
    }
}

impl Serialize for InteractionContext {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for InteractionContext {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;

        Ok(Self::from_i64(value))
    }
}

/// The documented interaction callback type: the `type` of an interaction
/// response.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionCallbackType {
    /// Type `1`: acknowledge a ping with a pong.
    Pong,
    /// Type `4`: respond with a message.
    ChannelMessageWithSource,
    /// Type `5`: acknowledge and edit a response later, with a loading state.
    DeferredChannelMessageWithSource,
    /// Type `6`: acknowledge and edit the original message later, without a
    /// loading state.
    DeferredUpdateMessage,
    /// Type `7`: edit the message the component was attached to.
    UpdateMessage,
    /// Type `8`: respond to an autocomplete interaction with choices.
    ApplicationCommandAutocompleteResult,
    /// Type `9`: respond with a modal.
    Modal,
    /// Type `10`: deprecated premium upgrade response.
    PremiumRequired,
    /// Type `12`: launch the app's Activity.
    LaunchActivity,
    /// A callback type Discord added later, preserved as sent.
    Unknown(i64),
}

impl InteractionCallbackType {
    /// Maps the documented integer callback type to an
    /// [`InteractionCallbackType`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::Pong,
            4 => Self::ChannelMessageWithSource,
            5 => Self::DeferredChannelMessageWithSource,
            6 => Self::DeferredUpdateMessage,
            7 => Self::UpdateMessage,
            8 => Self::ApplicationCommandAutocompleteResult,
            9 => Self::Modal,
            10 => Self::PremiumRequired,
            12 => Self::LaunchActivity,
            other => Self::Unknown(other),
        }
    }

    /// Returns the documented integer callback type.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Pong => 1,
            Self::ChannelMessageWithSource => 4,
            Self::DeferredChannelMessageWithSource => 5,
            Self::DeferredUpdateMessage => 6,
            Self::UpdateMessage => 7,
            Self::ApplicationCommandAutocompleteResult => 8,
            Self::Modal => 9,
            Self::PremiumRequired => 10,
            Self::LaunchActivity => 12,
            Self::Unknown(value) => value,
        }
    }
}

impl Serialize for InteractionCallbackType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for InteractionCallbackType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;

        Ok(Self::from_i64(value))
    }
}

/// The value of an authorizing integration owner: a snowflake, or `"0"` for
/// `GUILD_INSTALL` interactions triggered from a DM with the app's bot user.
///
/// Reference: `docs/receiving-and-responding.md` (Authorizing Integration
/// Owners Object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegrationOwnerId {
    /// The value `"0"`: the interaction happened in a DM with the app's bot
    /// user.
    BotDm,
    /// A snowflake, for example the guild id or the authorizing user id.
    Id(GenericId),
}

impl IntegrationOwnerId {
    /// Parses one documented value; unparseable values become `None` so an
    /// unexpected shape cannot fail the interaction.
    fn parse(value: &Value) -> Option<Self> {
        match value {
            Value::String(text) => {
                if text == "0" {
                    return Some(Self::BotDm);
                }

                text.parse::<u64>()
                    .ok()
                    .filter(|id| *id != 0)
                    .map(|id| Self::Id(GenericId::new(id)))
            }
            Value::Number(number) => number
                .as_u64()
                .filter(|id| *id != 0)
                .map(|id| Self::Id(GenericId::new(id))),
            _ => None,
        }
    }
}

impl Serialize for IntegrationOwnerId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::BotDm => serializer.serialize_str("0"),
            Self::Id(id) => id.serialize(serializer),
        }
    }
}

/// The documented `authorizing_integration_owners` dictionary: which
/// installation context an interaction was authorized for, and by which guild
/// or user.
///
/// Reference: `docs/receiving-and-responding.md` (Authorizing Integration
/// Owners Object).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthorizingIntegrationOwners {
    /// The `GUILD_INSTALL` (`"0"`) owner.
    pub guild_install: Option<IntegrationOwnerId>,
    /// The `USER_INSTALL` (`"1"`) owner.
    pub user_install: Option<IntegrationOwnerId>,
}

impl Serialize for AuthorizingIntegrationOwners {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        if let Some(owner) = &self.guild_install {
            map.serialize_entry("0", owner)?;
        }
        if let Some(owner) = &self.user_install {
            map.serialize_entry("1", owner)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for AuthorizingIntegrationOwners {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = HashMap::<String, Value>::deserialize(deserializer)?;
        let mut owners = Self::default();

        for (key, value) in raw {
            let Some(owner) = IntegrationOwnerId::parse(&value) else {
                continue;
            };

            match key.as_str() {
                "0" => owners.guild_install = Some(owner),
                "1" => owners.user_install = Some(owner),
                // An installation context this crate does not model: ignored
                // rather than failing the interaction (lenient by design).
                _ => {}
            }
        }

        Ok(owners)
    }
}

/// An interaction sent by Discord.
///
/// Deserialisation is lenient: an interaction type this crate does not route
/// becomes [`Interaction::Unknown`] rather than an error.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Object).
#[derive(Debug, Clone, PartialEq)]
pub enum Interaction {
    /// Type `1`: a ping interaction, typically an endpoint verification.
    Ping(PingInteraction),
    /// Type `2`: an application command invocation.
    Command(CommandInteraction),
    /// Type `3`: a message component invocation (button, select menu, ...).
    Component(ComponentInteraction),
    /// Type `4`: an application command autocomplete interaction.
    Autocomplete(AutocompleteInteraction),
    /// Type `5`: a modal submit interaction.
    ModalSubmit(ModalSubmitInteraction),
    /// An interaction type this crate does not model, kept as sent.
    Unknown(i64),
}

impl Interaction {
    /// Returns the documented interaction type.
    #[must_use]
    pub const fn kind(&self) -> InteractionType {
        match self {
            Self::Ping(_) => InteractionType::Ping,
            Self::Command(_) => InteractionType::ApplicationCommand,
            Self::Component(_) => InteractionType::MessageComponent,
            Self::Autocomplete(_) => InteractionType::ApplicationCommandAutocomplete,
            Self::ModalSubmit(_) => InteractionType::ModalSubmit,
            Self::Unknown(value) => InteractionType::Unknown(*value),
        }
    }
}

impl<'de> Deserialize<'de> for Interaction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(Value::as_i64)
            .ok_or_else(|| D::Error::missing_field("type"))?;

        match kind {
            1 => serde_json::from_value::<PingInteraction>(value)
                .map(Self::Ping)
                .map_err(D::Error::custom),
            2 => serde_json::from_value::<CommandInteraction>(value)
                .map(Self::Command)
                .map_err(D::Error::custom),
            3 => serde_json::from_value::<ComponentInteraction>(value)
                .map(Self::Component)
                .map_err(D::Error::custom),
            4 => serde_json::from_value::<AutocompleteInteraction>(value)
                .map(Self::Autocomplete)
                .map_err(D::Error::custom),
            5 => serde_json::from_value::<ModalSubmitInteraction>(value)
                .map(Self::ModalSubmit)
                .map_err(D::Error::custom),
            other => Ok(Self::Unknown(other)),
        }
    }
}

/// A ping interaction (type `1`).
///
/// The typed fields are the ones needed to acknowledge the ping with a pong
/// callback within the documented three second deadline.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Object) and
/// `docs/interactions-overview.md` (Acknowledging PING requests).
#[derive(Clone, PartialEq, Eq)]
pub struct PingInteraction {
    /// The interaction id.
    pub id: InteractionId,
    /// The id of the application this interaction is for.
    pub application_id: ApplicationId,
    /// The interaction token, valid for 15 minutes.
    pub token: String,
    /// The read-only interaction version; always `1`.
    pub version: i64,
}

impl fmt::Debug for PingInteraction {
    /// Formats the interaction without exposing its token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PingInteraction")
            .field("id", &self.id)
            .field("application_id", &self.application_id)
            .field("token", &"<redacted>")
            .field("version", &self.version)
            .finish()
    }
}

/// The `version` every interaction carries on the wire.
const fn default_version() -> i64 {
    1
}

/// The fields shared by every interaction that carries a `data` payload.
#[derive(Deserialize)]
struct InteractionWire {
    id: InteractionId,
    application_id: ApplicationId,
    token: String,
    #[serde(default = "default_version")]
    version: i64,
    #[serde(default)]
    guild_id: Option<GuildId>,
    #[serde(default)]
    channel_id: Option<ChannelId>,
    #[serde(default)]
    member: Option<Member>,
    #[serde(default)]
    user: Option<User>,
    #[serde(default)]
    app_permissions: Option<Permissions>,
    #[serde(default)]
    locale: Option<String>,
    #[serde(default)]
    guild_locale: Option<String>,
    #[serde(default)]
    context: Option<InteractionContext>,
    #[serde(default)]
    authorizing_integration_owners: Option<AuthorizingIntegrationOwners>,
    #[serde(default)]
    message: Option<Message>,
}

/// The ping-specific part of a type `1` interaction payload.
#[derive(Deserialize)]
struct PingInteractionWire {
    id: InteractionId,
    application_id: ApplicationId,
    token: String,
    #[serde(default = "default_version")]
    version: i64,
}

impl<'de> Deserialize<'de> for PingInteraction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = PingInteractionWire::deserialize(deserializer)?;

        Ok(Self {
            id: wire.id,
            application_id: wire.application_id,
            token: wire.token,
            version: wire.version,
        })
    }
}

/// An application command interaction (type `2`).
///
/// `user` is always the invoking user: in a guild Discord sends the member
/// object and this type is filled from `member.user`; in a DM it is the
/// top-level `user` field. Deserialisation accepts either shape.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Object,
/// Application Command Data Structure).
#[derive(Clone, PartialEq)]
pub struct CommandInteraction {
    /// The interaction id.
    pub id: InteractionId,
    /// The id of the application this interaction is for.
    pub application_id: ApplicationId,
    /// The interaction token, valid for 15 minutes.
    pub token: String,
    /// The read-only interaction version; always `1`.
    pub version: i64,
    /// The guild id, when the interaction happened in a guild.
    pub guild_id: Option<GuildId>,
    /// The channel id, when the interaction happened in a channel.
    pub channel_id: Option<ChannelId>,
    /// The invoking member, when the interaction happened in a guild.
    pub member: Option<Member>,
    /// The invoking user (`member.user` when sent in a guild).
    pub user: User,
    /// The command data.
    pub data: CommandData,
    /// The permissions the application has in the channel the interaction was
    /// sent in.
    pub app_permissions: Option<Permissions>,
    /// The invoking user's locale.
    pub locale: Option<String>,
    /// The guild's locale, when the interaction happened in a guild.
    pub guild_locale: Option<String>,
    /// The context the interaction was triggered from.
    pub context: Option<InteractionContext>,
    /// The installation contexts the interaction was authorized for.
    pub authorizing_integration_owners: Option<AuthorizingIntegrationOwners>,
}

impl fmt::Debug for CommandInteraction {
    /// Formats the interaction without exposing its token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandInteraction")
            .field("id", &self.id)
            .field("application_id", &self.application_id)
            .field("token", &"<redacted>")
            .field("version", &self.version)
            .field("guild_id", &self.guild_id)
            .field("channel_id", &self.channel_id)
            .field("member", &self.member)
            .field("user", &self.user)
            .field("data", &self.data)
            .field("app_permissions", &self.app_permissions)
            .field("locale", &self.locale)
            .field("guild_locale", &self.guild_locale)
            .field("context", &self.context)
            .field(
                "authorizing_integration_owners",
                &self.authorizing_integration_owners,
            )
            .finish()
    }
}

/// The command-specific part of a type `2` or type `4` interaction payload.
#[derive(Deserialize)]
struct CommandInteractionWire {
    #[serde(flatten)]
    common: InteractionWire,
    data: CommandData,
}

impl CommandInteraction {
    /// Fills in the invoking user from the member when the top-level `user`
    /// field is absent, as documented for guild interactions.
    fn from_wire(common: InteractionWire, data: CommandData) -> Result<Self, serde_json::Error> {
        let user = resolve_user(&common)?;

        Ok(Self {
            id: common.id,
            application_id: common.application_id,
            token: common.token,
            version: common.version,
            guild_id: common.guild_id,
            channel_id: common.channel_id,
            member: common.member,
            user,
            data,
            app_permissions: common.app_permissions,
            locale: common.locale,
            guild_locale: common.guild_locale,
            context: common.context,
            authorizing_integration_owners: common.authorizing_integration_owners,
        })
    }
}

impl<'de> Deserialize<'de> for CommandInteraction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = CommandInteractionWire::deserialize(deserializer)?;

        Self::from_wire(wire.common, wire.data).map_err(D::Error::custom)
    }
}

/// An application command autocomplete interaction (type `4`).
///
/// The invoking user rules are the same as [`CommandInteraction`]. The command
/// data's `options` can be partial: the option the user is typing is marked
/// `focused: true` and can be reached with
/// [`AutocompleteInteraction::focused_option`].
///
/// Reference: `docs/receiving-and-responding.md` (Application Command Data
/// Structure) and `docs/application-commands.md` (Autocomplete).
#[derive(Clone, PartialEq)]
pub struct AutocompleteInteraction {
    /// The interaction id.
    pub id: InteractionId,
    /// The id of the application this interaction is for.
    pub application_id: ApplicationId,
    /// The interaction token, valid for 15 minutes.
    pub token: String,
    /// The read-only interaction version; always `1`.
    pub version: i64,
    /// The guild id, when the interaction happened in a guild.
    pub guild_id: Option<GuildId>,
    /// The channel id, when the interaction happened in a channel.
    pub channel_id: Option<ChannelId>,
    /// The invoking member, when the interaction happened in a guild.
    pub member: Option<Member>,
    /// The invoking user (`member.user` when sent in a guild).
    pub user: User,
    /// The partial command data, including the focused option.
    pub data: CommandData,
    /// The permissions the application has in the channel the interaction was
    /// sent in.
    pub app_permissions: Option<Permissions>,
    /// The invoking user's locale.
    pub locale: Option<String>,
    /// The guild's locale, when the interaction happened in a guild.
    pub guild_locale: Option<String>,
    /// The context the interaction was triggered from.
    pub context: Option<InteractionContext>,
    /// The installation contexts the interaction was authorized for.
    pub authorizing_integration_owners: Option<AuthorizingIntegrationOwners>,
}

impl AutocompleteInteraction {
    /// Returns the option the user is currently typing into, if it was sent.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Application Command
    /// Interaction Data Option Structure, `focused`) and
    /// `docs/application-commands.md` (Autocomplete).
    #[must_use]
    pub fn focused_option(&self) -> Option<&CommandDataOption> {
        self.data.focused_option()
    }
}

impl fmt::Debug for AutocompleteInteraction {
    /// Formats the interaction without exposing its token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AutocompleteInteraction")
            .field("id", &self.id)
            .field("application_id", &self.application_id)
            .field("token", &"<redacted>")
            .field("version", &self.version)
            .field("guild_id", &self.guild_id)
            .field("channel_id", &self.channel_id)
            .field("member", &self.member)
            .field("user", &self.user)
            .field("data", &self.data)
            .field("app_permissions", &self.app_permissions)
            .field("locale", &self.locale)
            .field("guild_locale", &self.guild_locale)
            .field("context", &self.context)
            .field(
                "authorizing_integration_owners",
                &self.authorizing_integration_owners,
            )
            .finish()
    }
}

impl<'de> Deserialize<'de> for AutocompleteInteraction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = CommandInteractionWire::deserialize(deserializer)?;
        let user = resolve_user(&wire.common).map_err(D::Error::custom)?;
        let common = wire.common;

        Ok(Self {
            id: common.id,
            application_id: common.application_id,
            token: common.token,
            version: common.version,
            guild_id: common.guild_id,
            channel_id: common.channel_id,
            member: common.member,
            user,
            data: wire.data,
            app_permissions: common.app_permissions,
            locale: common.locale,
            guild_locale: common.guild_locale,
            context: common.context,
            authorizing_integration_owners: common.authorizing_integration_owners,
        })
    }
}

/// A message component interaction (type `3`).
///
/// The same `user`/`member` rules as [`CommandInteraction`] apply.
/// `message` is the message the component was attached to, when sent.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Object,
/// Message Component Data Structure).
#[derive(Clone, PartialEq)]
pub struct ComponentInteraction {
    /// The interaction id.
    pub id: InteractionId,
    /// The id of the application this interaction is for.
    pub application_id: ApplicationId,
    /// The interaction token, valid for 15 minutes.
    pub token: String,
    /// The read-only interaction version; always `1`.
    pub version: i64,
    /// The guild id, when the interaction happened in a guild.
    pub guild_id: Option<GuildId>,
    /// The channel id, when the interaction happened in a channel.
    pub channel_id: Option<ChannelId>,
    /// The invoking member, when the interaction happened in a guild.
    pub member: Option<Member>,
    /// The invoking user (`member.user` when sent in a guild).
    pub user: User,
    /// The component data.
    pub data: ComponentData,
    /// The message the component was attached to.
    pub message: Option<Message>,
    /// The permissions the application has in the channel the interaction was
    /// sent in.
    pub app_permissions: Option<Permissions>,
    /// The invoking user's locale.
    pub locale: Option<String>,
    /// The guild's locale, when the interaction happened in a guild.
    pub guild_locale: Option<String>,
    /// The context the interaction was triggered from.
    pub context: Option<InteractionContext>,
    /// The installation contexts the interaction was authorized for.
    pub authorizing_integration_owners: Option<AuthorizingIntegrationOwners>,
}

impl fmt::Debug for ComponentInteraction {
    /// Formats the interaction without exposing its token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentInteraction")
            .field("id", &self.id)
            .field("application_id", &self.application_id)
            .field("token", &"<redacted>")
            .field("version", &self.version)
            .field("guild_id", &self.guild_id)
            .field("channel_id", &self.channel_id)
            .field("member", &self.member)
            .field("user", &self.user)
            .field("data", &self.data)
            .field("message", &self.message)
            .field("app_permissions", &self.app_permissions)
            .field("locale", &self.locale)
            .field("guild_locale", &self.guild_locale)
            .field("context", &self.context)
            .field(
                "authorizing_integration_owners",
                &self.authorizing_integration_owners,
            )
            .finish()
    }
}

/// The component-specific part of a type `3` interaction payload.
#[derive(Deserialize)]
struct ComponentInteractionWire {
    #[serde(flatten)]
    common: InteractionWire,
    data: ComponentData,
}

impl<'de> Deserialize<'de> for ComponentInteraction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ComponentInteractionWire::deserialize(deserializer)?;
        let user = resolve_user(&wire.common).map_err(D::Error::custom)?;
        let common = wire.common;

        Ok(Self {
            id: common.id,
            application_id: common.application_id,
            token: common.token,
            version: common.version,
            guild_id: common.guild_id,
            channel_id: common.channel_id,
            member: common.member,
            user,
            data: wire.data,
            message: common.message,
            app_permissions: common.app_permissions,
            locale: common.locale,
            guild_locale: common.guild_locale,
            context: common.context,
            authorizing_integration_owners: common.authorizing_integration_owners,
        })
    }
}

/// A modal submit interaction (type `5`).
///
/// The same `user`/`member` rules as [`CommandInteraction`] apply. `message`
/// is the message the modal was opened from, when sent.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Object, Modal
/// Submit Data Structure).
#[derive(Clone, PartialEq)]
pub struct ModalSubmitInteraction {
    /// The interaction id.
    pub id: InteractionId,
    /// The id of the application this interaction is for.
    pub application_id: ApplicationId,
    /// The interaction token, valid for 15 minutes.
    pub token: String,
    /// The read-only interaction version; always `1`.
    pub version: i64,
    /// The guild id, when the interaction happened in a guild.
    pub guild_id: Option<GuildId>,
    /// The channel id, when the interaction happened in a channel.
    pub channel_id: Option<ChannelId>,
    /// The invoking member, when the interaction happened in a guild.
    pub member: Option<Member>,
    /// The invoking user (`member.user` when sent in a guild).
    pub user: User,
    /// The submitted modal data.
    pub data: ModalSubmitData,
    /// The message the modal was opened from.
    pub message: Option<Message>,
    /// The permissions the application has in the channel the interaction was
    /// sent in.
    pub app_permissions: Option<Permissions>,
    /// The invoking user's locale.
    pub locale: Option<String>,
    /// The guild's locale, when the interaction happened in a guild.
    pub guild_locale: Option<String>,
    /// The context the interaction was triggered from.
    pub context: Option<InteractionContext>,
    /// The installation contexts the interaction was authorized for.
    pub authorizing_integration_owners: Option<AuthorizingIntegrationOwners>,
}

impl fmt::Debug for ModalSubmitInteraction {
    /// Formats the interaction without exposing its token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModalSubmitInteraction")
            .field("id", &self.id)
            .field("application_id", &self.application_id)
            .field("token", &"<redacted>")
            .field("version", &self.version)
            .field("guild_id", &self.guild_id)
            .field("channel_id", &self.channel_id)
            .field("member", &self.member)
            .field("user", &self.user)
            .field("data", &self.data)
            .field("message", &self.message)
            .field("app_permissions", &self.app_permissions)
            .field("locale", &self.locale)
            .field("guild_locale", &self.guild_locale)
            .field("context", &self.context)
            .field(
                "authorizing_integration_owners",
                &self.authorizing_integration_owners,
            )
            .finish()
    }
}

/// The modal-specific part of a type `5` interaction payload.
#[derive(Deserialize)]
struct ModalSubmitInteractionWire {
    #[serde(flatten)]
    common: InteractionWire,
    data: ModalSubmitData,
}

impl<'de> Deserialize<'de> for ModalSubmitInteraction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = ModalSubmitInteractionWire::deserialize(deserializer)?;
        let user = resolve_user(&wire.common).map_err(D::Error::custom)?;
        let common = wire.common;

        Ok(Self {
            id: common.id,
            application_id: common.application_id,
            token: common.token,
            version: common.version,
            guild_id: common.guild_id,
            channel_id: common.channel_id,
            member: common.member,
            user,
            data: wire.data,
            message: common.message,
            app_permissions: common.app_permissions,
            locale: common.locale,
            guild_locale: common.guild_locale,
            context: common.context,
            authorizing_integration_owners: common.authorizing_integration_owners,
        })
    }
}

/// Returns the invoking user, preferring the top-level `user` field and falling
/// back to `member.user` for guild interactions.
fn resolve_user(common: &InteractionWire) -> Result<User, serde_json::Error> {
    common
        .user
        .clone()
        .or_else(|| common.member.as_ref().map(|member| member.user.clone()))
        .ok_or_else(|| {
            serde::de::Error::custom("interaction is missing both `user` and `member.user`")
        })
}

/// A registered command, as returned by the registration endpoint.
///
/// Reference: `docs/application-commands.md` (Application Command Structure).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Command {
    /// The command id.
    pub id: CommandId,
    /// The application id that owns the command.
    pub application_id: ApplicationId,
    /// The guild id, for guild-scoped commands.
    #[serde(default)]
    pub guild_id: Option<GuildId>,
    /// The command name.
    pub name: String,
    /// The command description.
    #[serde(default)]
    pub description: String,
    /// The command's options.
    #[serde(default)]
    pub options: Vec<CommandOption>,
    /// The default permissions required to use the command, as a decimal
    /// permission string on the wire.
    #[serde(default)]
    pub default_member_permissions: Option<Permissions>,
}

/// One option of a registered [`Command`].
///
/// Reference: `docs/application-commands.md` (Application Command Option
/// Structure).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CommandOption {
    /// The option type, `type` on the wire.
    #[serde(rename = "type")]
    pub kind: CommandOptionKind,
    /// The option name.
    pub name: String,
    /// The option description.
    pub description: String,
    /// Whether the option is required.
    #[serde(default)]
    pub required: Option<bool>,
    /// Whether autocomplete is enabled for this option.
    ///
    /// Reference: `docs/application-commands.md` (Application Command Option
    /// Structure, `autocomplete`).
    #[serde(default)]
    pub autocomplete: Option<bool>,
    /// The minimum value, for integer and number options.
    #[serde(default)]
    pub min_value: Option<Value>,
    /// The maximum value, for integer and number options.
    #[serde(default)]
    pub max_value: Option<Value>,
}

/// The kind of an application command, `data.type` for command interactions.
///
/// Reference: `docs/application-commands.md` (Application Command Types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandKind {
    /// Type `1`: a chat input (slash) command.
    ChatInput,
    /// Type `2`: a user context menu command.
    User,
    /// Type `3`: a message context menu command.
    Message,
    /// Type `4`: a primary entry point command, which launches the app's
    /// Activity.
    PrimaryEntryPoint,
}

impl CommandKind {
    /// Maps the documented integer `type` value to a [`CommandKind`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::ChatInput),
            2 => Some(Self::User),
            3 => Some(Self::Message),
            4 => Some(Self::PrimaryEntryPoint),
            _ => None,
        }
    }

    /// Returns the documented integer `type` value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::ChatInput => 1,
            Self::User => 2,
            Self::Message => 3,
            Self::PrimaryEntryPoint => 4,
        }
    }
}

impl Serialize for CommandKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for CommandKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl Visitor<'_> for KindVisitor {
            type Value = CommandKind;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an application command type integer (1 through 4)")
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                CommandKind::from_i64(value)
                    .ok_or_else(|| E::custom(format!("unknown command type {value}")))
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                i64::try_from(value)
                    .ok()
                    .and_then(CommandKind::from_i64)
                    .ok_or_else(|| E::custom(format!("unknown command type {value}")))
            }
        }

        deserializer.deserialize_i64(KindVisitor)
    }
}

/// The `data` object of an application command interaction (type `2`) or an
/// autocomplete interaction (type `4`).
///
/// Reference: `docs/receiving-and-responding.md` (Application Command Data
/// Structure).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CommandData {
    /// The command id.
    pub id: CommandId,
    /// The command name.
    pub name: String,
    /// The command kind, `type` on the wire.
    #[serde(rename = "type")]
    pub kind: CommandKind,
    /// The resolved users, members, roles, channels, messages and attachments
    /// referenced by the supplied options.
    #[serde(default)]
    pub resolved: Option<ResolvedData>,
    /// The supplied options; partial for an autocomplete interaction.
    #[serde(default)]
    pub options: Vec<CommandDataOption>,
    /// The guild id a context menu command was used in, when returned.
    #[serde(default)]
    pub guild_id: Option<GuildId>,
    /// The target id of a context menu command (a user or message id).
    #[serde(default)]
    pub target_id: Option<GenericId>,
}

impl CommandData {
    /// Returns the option the user is currently typing into, if it was sent.
    ///
    /// Subcommand and subcommand-group options are searched recursively.
    ///
    /// Reference: `docs/application-commands.md` (Autocomplete).
    #[must_use]
    pub fn focused_option(&self) -> Option<&CommandDataOption> {
        find_focused(&self.options)
    }
}

/// Returns the first option (searching into subcommands) marked `focused`.
fn find_focused(options: &[CommandDataOption]) -> Option<&CommandDataOption> {
    for option in options {
        if option.focused {
            return Some(option);
        }

        let (CommandOptionValue::Subcommand(nested) | CommandOptionValue::SubcommandGroup(nested)) =
            &option.value
        else {
            continue;
        };

        if let Some(focused) = find_focused(nested) {
            return Some(focused);
        }
    }

    None
}

/// The resolved entities referenced by the supplied command options.
///
/// The maps are keyed by id, as documented; every map is lenient so one
/// malformed entry cannot fail the whole interaction.
///
/// Reference: `docs/receiving-and-responding.md` (Resolved Data Structure).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResolvedData {
    /// The resolved users, keyed by user id.
    #[serde(
        default,
        deserialize_with = "deserialize_lenient_map",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub users: HashMap<UserId, User>,
    /// The resolved partial members, keyed by user id.
    #[serde(
        default,
        deserialize_with = "deserialize_lenient_map",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub members: HashMap<UserId, PartialMember>,
    /// The resolved roles, keyed by role id.
    #[serde(
        default,
        deserialize_with = "deserialize_lenient_map",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub roles: HashMap<RoleId, Role>,
    /// The resolved partial channels, keyed by channel id.
    #[serde(
        default,
        deserialize_with = "deserialize_lenient_map",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub channels: HashMap<ChannelId, Channel>,
    /// The resolved partial messages, keyed by message id.
    #[serde(
        default,
        deserialize_with = "deserialize_lenient_map",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub messages: HashMap<MessageId, Message>,
    /// The resolved attachments, keyed by attachment id.
    #[serde(
        default,
        deserialize_with = "deserialize_lenient_map",
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub attachments: HashMap<AttachmentId, Attachment>,
}

impl ResolvedData {
    /// Returns the resolved user, if it was included.
    #[must_use]
    pub fn user(&self, id: UserId) -> Option<&User> {
        self.users.get(&id)
    }

    /// Returns the resolved partial member, if it was included.
    #[must_use]
    pub fn member(&self, id: UserId) -> Option<&PartialMember> {
        self.members.get(&id)
    }

    /// Returns the resolved role, if it was included.
    #[must_use]
    pub fn role(&self, id: RoleId) -> Option<&Role> {
        self.roles.get(&id)
    }

    /// Returns the resolved partial channel, if it was included.
    #[must_use]
    pub fn channel(&self, id: ChannelId) -> Option<&Channel> {
        self.channels.get(&id)
    }

    /// Returns the resolved partial message, if it was included.
    #[must_use]
    pub fn message(&self, id: MessageId) -> Option<&Message> {
        self.messages.get(&id)
    }

    /// Returns the resolved attachment, if it was included.
    #[must_use]
    pub fn attachment(&self, id: AttachmentId) -> Option<&Attachment> {
        self.attachments.get(&id)
    }
}

/// A partial member included in [`ResolvedData::members`]: the documented
/// member object without `user`, `deaf` and `mute`.
///
/// Reference: `docs/receiving-and-responding.md` (Resolved Data Structure,
/// footnote on partial members).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartialMember {
    /// The member's guild nickname.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    /// The ids of the roles assigned to this member.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<RoleId>,
    /// When the user joined the guild.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<crate::timestamp::Timestamp>,
    /// Whether the user has not yet passed Membership Screening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<bool>,
    /// The member's total permissions in the channel, when included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    /// When the member started boosting the guild.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub premium_since: Option<crate::timestamp::Timestamp>,
    /// The member's guild avatar hash, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    /// The member's guild banner hash, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    /// The member's guild flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Deserialises a JSON object into a map, skipping entries whose value does not
/// match the documented shape.
///
/// This keeps interaction deserialisation lenient: one malformed resolved
/// entity cannot fail the whole interaction.
fn deserialize_lenient_map<'de, Key, Item, D>(
    deserializer: D,
) -> Result<HashMap<Key, Item>, D::Error>
where
    D: Deserializer<'de>,
    Key: Deserialize<'de> + std::hash::Hash + Eq,
    Item: DeserializeOwned,
{
    let raw = HashMap::<Key, Value>::deserialize(deserializer)?;

    Ok(raw
        .into_iter()
        .filter_map(|(key, value)| serde_json::from_value(value).ok().map(|value| (key, value)))
        .collect())
}

/// The type of a command option, the integer `type` of an option object.
///
/// Reference: `docs/application-commands.md` (Application Command Option Type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandOptionKind {
    /// Type `1`: a subcommand.
    Subcommand,
    /// Type `2`: a group of subcommands.
    SubcommandGroup,
    /// Type `3`: a string option.
    Str,
    /// Type `4`: an integer option.
    Integer,
    /// Type `5`: a boolean option.
    Boolean,
    /// Type `6`: a user option.
    User,
    /// Type `7`: a channel option.
    Channel,
    /// Type `8`: a role option.
    Role,
    /// Type `9`: a mentionable (user or role) option.
    Mentionable,
    /// Type `10`: a floating-point number option.
    Number,
    /// Type `11`: an attachment option.
    Attachment,
}

impl CommandOptionKind {
    /// Maps the documented integer `type` value to a [`CommandOptionKind`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Option<Self> {
        match value {
            1 => Some(Self::Subcommand),
            2 => Some(Self::SubcommandGroup),
            3 => Some(Self::Str),
            4 => Some(Self::Integer),
            5 => Some(Self::Boolean),
            6 => Some(Self::User),
            7 => Some(Self::Channel),
            8 => Some(Self::Role),
            9 => Some(Self::Mentionable),
            10 => Some(Self::Number),
            11 => Some(Self::Attachment),
            _ => None,
        }
    }

    /// Returns the documented integer `type` value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Subcommand => 1,
            Self::SubcommandGroup => 2,
            Self::Str => 3,
            Self::Integer => 4,
            Self::Boolean => 5,
            Self::User => 6,
            Self::Channel => 7,
            Self::Role => 8,
            Self::Mentionable => 9,
            Self::Number => 10,
            Self::Attachment => 11,
        }
    }
}

impl Serialize for CommandOptionKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for CommandOptionKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl Visitor<'_> for KindVisitor {
            type Value = CommandOptionKind;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a command option type integer (1 through 11)")
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                CommandOptionKind::from_i64(value)
                    .ok_or_else(|| E::custom(format!("unknown command option type {value}")))
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                i64::try_from(value)
                    .ok()
                    .and_then(CommandOptionKind::from_i64)
                    .ok_or_else(|| E::custom(format!("unknown command option type {value}")))
            }
        }

        deserializer.deserialize_i64(KindVisitor)
    }
}

/// The value of a supplied command option.
///
/// The variant depends on the option's [`CommandOptionKind`]; option values are
/// a tagged union on the Discord wire, so the type is carried alongside the
/// value instead of being inferred.
///
/// Reference: `docs/receiving-and-responding.md` (Application Command
/// Interaction Data Option Structure).
#[derive(Debug, Clone, PartialEq)]
pub enum CommandOptionValue {
    /// A string value (option type `3`).
    String(String),
    /// An integer value (option type `4`).
    Integer(i64),
    /// A boolean value (option type `5`).
    Boolean(bool),
    /// A user id (option type `6`).
    User(UserId),
    /// A channel id (option type `7`).
    Channel(ChannelId),
    /// A role id (option type `8`).
    Role(RoleId),
    /// A user or role id (option type `9`).
    Mentionable(GenericId),
    /// A floating-point number value (option type `10`).
    Number(f64),
    /// An attachment id (option type `11`).
    Attachment(AttachmentId),
    /// The retained options of a subcommand (option type `1`).
    Subcommand(Vec<CommandDataOption>),
    /// The retained options of a subcommand group (option type `2`).
    SubcommandGroup(Vec<CommandDataOption>),
}

impl CommandOptionValue {
    /// Returns the value as a string, when it is a string option.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the value as an integer, when it is an integer option.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the value as a boolean, when it is a boolean option.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the value as a number, for number options and integer options.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            // `serde_json::Number` performs the documented integer-to-number
            // conversion without a precision-losing `as` cast in this crate.
            Self::Integer(value) => serde_json::Number::from(*value).as_f64(),
            _ => None,
        }
    }

    /// Returns the value as a user id, when it is a user option.
    #[must_use]
    pub fn as_user_id(&self) -> Option<UserId> {
        match self {
            Self::User(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the value as a channel id, when it is a channel option.
    #[must_use]
    pub fn as_channel_id(&self) -> Option<ChannelId> {
        match self {
            Self::Channel(value) => Some(*value),
            _ => None,
        }
    }

    /// Returns the value as a role id, when it is a role option.
    #[must_use]
    pub fn as_role_id(&self) -> Option<RoleId> {
        match self {
            Self::Role(value) => Some(*value),
            _ => None,
        }
    }
}

/// One supplied command option: a name, a kind and a value.
///
/// Reference: `docs/receiving-and-responding.md` (Application Command
/// Interaction Data Option Structure).
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDataOption {
    /// The option name.
    pub name: String,
    /// The option type.
    pub kind: CommandOptionKind,
    /// The supplied value.
    pub value: CommandOptionValue,
    /// Whether this is the option the user is currently typing into.
    ///
    /// Only set by autocomplete interactions.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Application Command
    /// Interaction Data Option Structure, `focused`).
    pub focused: bool,
}

/// The intermediate option object on the wire: the value is typed by `type`.
#[derive(Deserialize)]
struct CommandDataOptionWire {
    name: String,
    #[serde(rename = "type")]
    kind: CommandOptionKind,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    options: Option<Vec<CommandDataOption>>,
    #[serde(default)]
    focused: bool,
}

impl<'de> Deserialize<'de> for CommandDataOption {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = CommandDataOptionWire::deserialize(deserializer)?;
        let value = option_value(&wire).map_err(D::Error::custom)?;

        Ok(Self {
            name: wire.name,
            kind: wire.kind,
            value,
            focused: wire.focused,
        })
    }
}

/// Converts the wire `value`/`options` fields to a [`CommandOptionValue`] based
/// on the declared option kind.
fn option_value(wire: &CommandDataOptionWire) -> Result<CommandOptionValue, String> {
    use CommandOptionKind as Kind;

    let value = wire.value.as_ref();
    let missing = || format!("option {:?} is missing its value", wire.name);

    match wire.kind {
        Kind::Subcommand => Ok(CommandOptionValue::Subcommand(
            wire.options.clone().unwrap_or_default(),
        )),
        Kind::SubcommandGroup => Ok(CommandOptionValue::SubcommandGroup(
            wire.options.clone().unwrap_or_default(),
        )),
        Kind::Str => value
            .and_then(Value::as_str)
            .map(|value| CommandOptionValue::String(value.to_owned()))
            .ok_or_else(missing),
        Kind::Integer => value
            .and_then(Value::as_i64)
            .map(CommandOptionValue::Integer)
            .ok_or_else(missing),
        Kind::Boolean => value
            .and_then(Value::as_bool)
            .map(CommandOptionValue::Boolean)
            .ok_or_else(missing),
        Kind::User => snowflake(value).map(CommandOptionValue::User),
        Kind::Channel => snowflake(value).map(CommandOptionValue::Channel),
        Kind::Role => snowflake(value).map(CommandOptionValue::Role),
        Kind::Mentionable => snowflake(value).map(CommandOptionValue::Mentionable),
        Kind::Number => value
            .and_then(Value::as_f64)
            .map(CommandOptionValue::Number)
            .ok_or_else(missing),
        Kind::Attachment => snowflake(value).map(CommandOptionValue::Attachment),
    }
}

/// Parses a snowflake option value (a decimal string, or an integer as
/// tolerated by the id deserialiser).
fn snowflake<Kind>(value: Option<&Value>) -> Result<Id<Kind>, String> {
    let value = value.ok_or_else(|| "snowflake option is missing its value".to_owned())?;

    serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid snowflake option value: {error}"))
}

/// The `data` object of a message component interaction (type `3`).
///
/// Reference: `docs/receiving-and-responding.md` (Message Component Data
/// Structure).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ComponentData {
    /// The `custom_id` of the component that was interacted with.
    pub custom_id: String,
    /// The component type integer, `component_type` on the wire.
    #[serde(rename = "component_type")]
    pub component_kind: i64,
    /// The 32-bit component id, when sent.
    #[serde(default)]
    pub id: Option<u32>,
    /// The values of a select menu interaction.
    #[serde(default)]
    pub values: Vec<String>,
    /// The resolved entities from the selected options.
    #[serde(default)]
    pub resolved: Option<ResolvedData>,
}

impl ComponentData {
    /// Returns the interacted component's type as a
    /// [`ComponentKind`](crate::model::component::ComponentKind).
    ///
    /// Reference: `docs/components-reference.md` (Component Types).
    #[must_use]
    pub const fn kind(&self) -> crate::model::component::ComponentKind {
        crate::model::component::ComponentKind::from_i64(self.component_kind)
    }
}

/// The `data` object of a modal submit interaction (type `5`).
///
/// Reference: `docs/receiving-and-responding.md` (Modal Submit Data Structure).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ModalSubmitData {
    /// The `custom_id` the modal was opened with.
    pub custom_id: String,
    /// The submitted component values, nested inside their action rows or
    /// labels.
    ///
    /// Reference: `docs/components-reference.md` (each component's Interaction
    /// Response Structure) and
    /// `docs/receiving-and-responding.md` (Component Interaction Response
    /// Structures).
    #[serde(default)]
    pub components: Vec<Component>,
    /// The resolved entities from the submitted select menus.
    #[serde(default)]
    pub resolved: Option<ResolvedData>,
}

impl ModalSubmitData {
    /// Returns the first submitted component with the given `custom_id`.
    ///
    /// The search walks action rows, labels, sections and containers, where the
    /// submitted components can be nested.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Modal Submit Data
    /// Structure).
    #[must_use]
    pub fn component(&self, custom_id: &str) -> Option<&Component> {
        find_component(&self.components, custom_id)
    }
}

/// Searches nested components for one with `custom_id`.
fn find_component<'a>(components: &'a [Component], custom_id: &str) -> Option<&'a Component> {
    for component in components {
        if component.custom_id() == Some(custom_id) {
            return Some(component);
        }

        match component {
            Component::ActionRow(row) => {
                if let Some(found) = find_component(&row.components, custom_id) {
                    return Some(found);
                }
            }
            Component::Label(label) => {
                if let Some(found) =
                    find_component(std::slice::from_ref(&label.component), custom_id)
                {
                    return Some(found);
                }
            }
            Component::Section(section) => {
                if let Some(found) = find_component(&section.components, custom_id) {
                    return Some(found);
                }
            }
            Component::Container(container) => {
                if let Some(found) = find_component(&container.components, custom_id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }

    None
}

/// The Message Interaction Object attached to a message sent as an interaction
/// response.
///
/// Reference: `docs/receiving-and-responding.md` (Message Interaction Object).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageInteraction {
    /// The interaction id.
    pub id: InteractionId,
    /// The interaction type, `type` on the wire.
    #[serde(rename = "type")]
    pub kind: InteractionType,
    /// The name of the application command, including subcommands.
    pub name: String,
    /// The user who invoked the interaction.
    pub user: User,
    /// The member who invoked the interaction, in a guild.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<PartialMember>,
}

/// The body returned by a callback request made with `with_response=true`.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Response
/// Object).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InteractionCallbackResponse {
    /// The interaction the response belongs to.
    pub interaction: InteractionCallback,
    /// The resource the callback created, when there is one.
    #[serde(default)]
    pub resource: Option<InteractionCallbackResource>,
}

impl InteractionCallbackResponse {
    /// Returns the message the callback created, if it created one.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback
    /// Resource Object) — the message is only present for callback types
    /// `CHANNEL_MESSAGE_WITH_SOURCE` and `UPDATE_MESSAGE`.
    #[must_use]
    pub fn created_message(&self) -> Option<&Message> {
        self.resource
            .as_ref()
            .and_then(|resource| resource.message.as_ref())
    }
}

/// The `interaction` object of an [`InteractionCallbackResponse`].
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Object).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InteractionCallback {
    /// The interaction id.
    pub id: InteractionId,
    /// The interaction type, `type` on the wire.
    #[serde(rename = "type")]
    pub kind: InteractionType,
    /// The instance id of a launched or joined Activity.
    #[serde(default)]
    pub activity_instance_id: Option<String>,
    /// The id of the message the interaction created.
    #[serde(default)]
    pub response_message_id: Option<MessageId>,
    /// Whether the created message is in a loading state.
    #[serde(default)]
    pub response_message_loading: Option<bool>,
    /// Whether the created message is ephemeral.
    #[serde(default)]
    pub response_message_ephemeral: Option<bool>,
}

/// The `resource` object of an [`InteractionCallbackResponse`].
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Resource
/// Object).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InteractionCallbackResource {
    /// The interaction callback type that created the resource.
    #[serde(rename = "type")]
    pub kind: InteractionCallbackType,
    /// The Activity launched by the interaction, for `LAUNCH_ACTIVITY`.
    #[serde(default)]
    pub activity_instance: Option<InteractionCallbackActivityInstance>,
    /// The message created by the interaction, for callback types `4` and `7`.
    #[serde(default)]
    pub message: Option<Message>,
}

/// The Activity instance resource of an [`InteractionCallbackResource`].
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Activity
/// Instance Resource).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InteractionCallbackActivityInstance {
    /// The instance id of the launched or joined Activity.
    pub id: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn interaction(value: Value) -> Interaction {
        serde_json::from_value(value).expect("an interaction fixture parses")
    }

    fn option(value: Value) -> CommandDataOption {
        serde_json::from_value(value).expect("option fixture should parse")
    }

    /// A minimal documented command interaction payload.
    fn command_payload() -> Value {
        json!({
            "type": 2,
            "id": "100",
            "application_id": "200",
            "token": "interaction-token",
            "version": 1,
            "guild_id": "300",
            "channel_id": "400",
            "member": {
                "user": {
                    "id": "500",
                    "username": "invoker",
                    "discriminator": "0"
                },
                "nick": "Invoker",
                "roles": ["600"],
                "joined_at": "2020-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "permissions": "2048",
                "pending": false
            },
            "data": {
                "id": "700",
                "name": "ban",
                "type": 1,
                "options": [
                    {"name": "target", "type": 6, "value": "500"},
                    {"name": "reason", "type": 3, "value": "spam"}
                ]
            },
            "app_permissions": "2048",
            "locale": "en-US",
            "guild_locale": "en-US",
            "context": 0,
            "authorizing_integration_owners": {"0": "300"}
        })
    }

    #[test]
    fn parses_a_ping_interaction_with_its_acknowledgement_fields() {
        let interaction = interaction(json!({
            "type": 1,
            "id": "100",
            "application_id": "200",
            "token": "interaction-token",
            "version": 1
        }));

        let Interaction::Ping(ping) = interaction else {
            panic!("expected a ping interaction");
        };
        assert_eq!(ping.id, InteractionId::new(100));
        assert_eq!(ping.application_id, ApplicationId::new(200));
        assert_eq!(ping.token, "interaction-token");
        assert_eq!(ping.version, 1);
    }

    #[test]
    fn a_ping_without_a_version_defaults_it_to_one() {
        let ping: PingInteraction = serde_json::from_value(json!({
            "id": "1",
            "application_id": "2",
            "token": "t"
        }))
        .expect("ping payload");

        assert_eq!(ping.version, 1);
    }

    #[test]
    fn interaction_kinds_map_the_documented_types() {
        let cases = [
            (
                interaction(json!({
                    "type": 1, "id": "1", "application_id": "2", "token": "t"
                })),
                InteractionType::Ping,
            ),
            (
                interaction(command_payload()),
                InteractionType::ApplicationCommand,
            ),
            (
                interaction(json!({
                    "type": 3, "id": "1", "application_id": "2", "token": "t",
                    "user": {"id": "3", "username": "u"},
                    "data": {"custom_id": "c", "component_type": 2}
                })),
                InteractionType::MessageComponent,
            ),
            (
                interaction(json!({
                    "type": 4, "id": "1", "application_id": "2", "token": "t",
                    "user": {"id": "3", "username": "u"},
                    "data": {"id": "4", "name": "n", "type": 1, "options": []}
                })),
                InteractionType::ApplicationCommandAutocomplete,
            ),
            (
                interaction(json!({
                    "type": 5, "id": "1", "application_id": "2", "token": "t",
                    "user": {"id": "3", "username": "u"},
                    "data": {"custom_id": "c", "components": []}
                })),
                InteractionType::ModalSubmit,
            ),
        ];

        for (interaction, kind) in cases {
            assert_eq!(interaction.kind(), kind);
            assert_eq!(kind.as_i64(), interaction.kind().as_i64());
        }

        assert_eq!(InteractionType::from_i64(42), InteractionType::Unknown(42));
        assert_eq!(InteractionType::Unknown(42).as_i64(), 42);
    }

    #[test]
    fn unknown_interaction_types_become_unknown_instead_of_erroring() {
        let interaction = interaction(json!({"type": 99}));

        assert_eq!(interaction, Interaction::Unknown(99));
    }

    #[test]
    fn parses_a_guild_command_interaction_using_member_user() {
        let Interaction::Command(command) = interaction(command_payload()) else {
            panic!("expected a command interaction");
        };

        assert_eq!(command.id, InteractionId::new(100));
        assert_eq!(command.application_id, ApplicationId::new(200));
        assert_eq!(command.token, "interaction-token");
        assert_eq!(command.version, 1);
        assert_eq!(command.guild_id, Some(GuildId::new(300)));
        assert_eq!(command.channel_id, Some(ChannelId::new(400)));
        assert_eq!(command.user.id, UserId::new(500));
        assert_eq!(command.user.username, "invoker");
        let member = command.member.expect("member");
        assert_eq!(member.display_name(), "Invoker");
        assert_eq!(member.permissions, Some(Permissions::SEND_MESSAGES));
        assert_eq!(command.app_permissions, Some(Permissions::SEND_MESSAGES));
        assert_eq!(command.locale.as_deref(), Some("en-US"));
        assert_eq!(command.guild_locale.as_deref(), Some("en-US"));
        assert_eq!(command.context, Some(InteractionContext::Guild));
        assert_eq!(
            command.authorizing_integration_owners,
            Some(AuthorizingIntegrationOwners {
                guild_install: Some(IntegrationOwnerId::Id(GenericId::new(300))),
                user_install: None,
            })
        );
        assert_eq!(command.data.name, "ban");
        assert_eq!(command.data.kind, CommandKind::ChatInput);
        assert!(command.data.resolved.is_none());
        assert_eq!(command.data.options.len(), 2);
        assert_eq!(
            command.data.options[0].value.as_user_id(),
            Some(UserId::new(500))
        );
        assert_eq!(command.data.options[1].value.as_str(), Some("spam"));
        assert!(!command.data.options[0].focused);
    }

    #[test]
    fn parses_a_documented_message_command_interaction_with_resolved_messages() {
        let Interaction::Command(command) = interaction(json!({
            "type": 2,
            "id": "867793873336926249",
            "application_id": "775799577604522054",
            "token": "UNIQUE_TOKEN",
            "version": 1,
            "guild_id": "772904309264089089",
            "channel_id": "772908445358620702",
            "member": {
                "user": {"id": "167348773423415296", "username": "ian"},
                "roles": [],
                "joined_at": "2020-11-02T20:46:57.364000+00:00",
                "deaf": false,
                "mute": false
            },
            "data": {
                "id": "866818195033292851",
                "name": "context-menu-message-2",
                "type": 3,
                "target_id": "867793854505943041",
                "resolved": {
                    "messages": {
                        "867793854505943041": {
                            "attachments": [],
                            "author": {"id": "167348773423415296", "username": "ian"},
                            "channel_id": "772908445358620702",
                            "components": [],
                            "content": "some message",
                            "edited_timestamp": null,
                            "embeds": [],
                            "flags": 0,
                            "id": "867793854505943041",
                            "mention_everyone": false,
                            "pinned": false,
                            "timestamp": "2021-07-22T15:42:57.744000+00:00",
                            "tts": false,
                            "type": 0
                        }
                    }
                }
            }
        })) else {
            panic!("expected a command interaction");
        };

        assert_eq!(command.data.kind, CommandKind::Message);
        assert_eq!(
            command.data.target_id,
            Some(GenericId::new(867_793_854_505_943_041))
        );
        let resolved = command.data.resolved.expect("resolved data");
        assert_eq!(
            resolved
                .message(MessageId::new(867_793_854_505_943_041))
                .map(|message| message.content.as_str()),
            Some("some message")
        );
    }

    #[test]
    fn parses_an_entry_point_command_interaction() {
        let Interaction::Command(command) = interaction(json!({
            "type": 2,
            "id": "100",
            "application_id": "200",
            "token": "token",
            "user": {"id": "500", "username": "invoker"},
            "data": {"id": "700", "name": "launch", "type": 4}
        })) else {
            panic!("expected a command interaction");
        };

        assert_eq!(command.data.kind, CommandKind::PrimaryEntryPoint);
        assert_eq!(CommandKind::PrimaryEntryPoint.as_i64(), 4);
    }

    #[test]
    fn parses_a_direct_message_command_interaction_using_the_top_level_user() {
        let Interaction::Command(command) = interaction(json!({
            "type": 2,
            "id": "100",
            "application_id": "200",
            "token": "token",
            "user": {"id": "500", "username": "invoker", "discriminator": "0"},
            "data": {
                "id": "700",
                "name": "ping",
                "type": 1
            },
            "context": 1,
            "authorizing_integration_owners": {"0": "0", "1": "500"}
        })) else {
            panic!("expected a command interaction");
        };

        assert_eq!(command.version, 1);
        assert_eq!(command.guild_id, None);
        assert_eq!(command.channel_id, None);
        assert_eq!(command.member, None);
        assert_eq!(command.user.id, UserId::new(500));
        assert!(command.data.options.is_empty());
        assert_eq!(command.context, Some(InteractionContext::BotDm));
        assert_eq!(
            command.authorizing_integration_owners,
            Some(AuthorizingIntegrationOwners {
                guild_install: Some(IntegrationOwnerId::BotDm),
                user_install: Some(IntegrationOwnerId::Id(GenericId::new(500))),
            })
        );
    }

    #[test]
    fn an_unknown_context_is_preserved() {
        let Interaction::Command(command) = interaction(json!({
            "type": 2,
            "id": "1",
            "application_id": "2",
            "token": "t",
            "user": {"id": "3", "username": "u"},
            "data": {"id": "4", "name": "n", "type": 1},
            "context": 7
        })) else {
            panic!("expected a command interaction");
        };

        assert_eq!(command.context, Some(InteractionContext::Unknown(7)));
        assert_eq!(InteractionContext::Unknown(7).as_i64(), 7);
    }

    #[test]
    fn parses_a_component_interaction_with_its_resolved_data_and_message() {
        let Interaction::Component(component) = interaction(json!({
            "type": 3,
            "id": "100",
            "application_id": "200",
            "token": "token",
            "version": 1,
            "guild_id": "300",
            "channel_id": "400",
            "user": {"id": "500", "username": "invoker"},
            "data": {
                "custom_id": "user_select",
                "component_type": 5,
                "id": 2,
                "values": ["1111111111111111111"],
                "resolved": {
                    "members": {
                        "1111111111111111111": {
                            "nick": "Ant",
                            "roles": ["2222222222222222222"],
                            "joined_at": "2025-05-16T22:51:16.692000+00:00",
                            "pending": false,
                            "permissions": "2248473465835073"
                        }
                    },
                    "users": {
                        "1111111111111111111": {
                            "id": "1111111111111111111",
                            "username": "ExampleBot",
                            "discriminator": "9062",
                            "bot": true
                        }
                    }
                }
            },
            "message": {
                "id": "1000",
                "channel_id": "400",
                "author": {"id": "200", "username": "bot"},
                "timestamp": "2026-10-22T17:00:00.000000+00:00",
                "content": "Pick a user",
                "pinned": false,
                "type": 20,
                "components": [
                    {
                        "type": 1,
                        "components": [
                            {"type": 5, "custom_id": "user_select"}
                        ]
                    }
                ]
            },
            "app_permissions": "2048",
            "locale": "en-US",
            "guild_locale": "en-US",
            "context": 0,
            "authorizing_integration_owners": {"0": "300"}
        })) else {
            panic!("expected a component interaction");
        };

        assert_eq!(component.user.id, UserId::new(500));
        assert_eq!(component.data.custom_id, "user_select");
        assert_eq!(component.data.component_kind, 5);
        assert_eq!(
            component.data.kind(),
            crate::model::component::ComponentKind::UserSelect
        );
        assert_eq!(component.data.id, Some(2));
        assert_eq!(
            component.data.values,
            vec!["1111111111111111111".to_owned()]
        );
        assert_eq!(component.app_permissions, Some(Permissions::SEND_MESSAGES));
        assert_eq!(component.locale.as_deref(), Some("en-US"));

        let resolved = component.data.resolved.as_ref().expect("resolved data");
        let member = resolved
            .member(UserId::new(1_111_111_111_111_111_111))
            .expect("resolved member");
        assert_eq!(member.nick.as_deref(), Some("Ant"));
        assert_eq!(member.roles.len(), 1);
        assert_eq!(member.pending, Some(false));
        let user = resolved
            .user(UserId::new(1_111_111_111_111_111_111))
            .expect("resolved user");
        assert_eq!(user.username, "ExampleBot");
        assert!(user.bot);

        let message = component.message.expect("the interacted message");
        assert_eq!(message.content, "Pick a user");
        assert_eq!(message.components.len(), 1);
    }

    #[test]
    fn component_interaction_defaults_empty_values_and_optional_fields() {
        let Interaction::Component(component) = interaction(json!({
            "type": 3,
            "id": "1",
            "application_id": "2",
            "token": "t",
            "user": {"id": "3", "username": "u"},
            "data": {"custom_id": "button", "component_type": 2}
        })) else {
            panic!("expected a component interaction");
        };

        assert!(component.data.values.is_empty());
        assert_eq!(component.data.id, None);
        assert!(component.data.resolved.is_none());
        assert!(component.message.is_none());
        assert_eq!(component.version, 1);
        assert!(component.context.is_none());
    }

    #[test]
    fn parses_an_autocomplete_interaction_and_exposes_the_focused_option() {
        let Interaction::Autocomplete(autocomplete) = interaction(json!({
            "type": 4,
            "id": "100",
            "application_id": "200",
            "token": "token",
            "guild_id": "300",
            "channel_id": "400",
            "user": {"id": "500", "username": "invoker"},
            "data": {
                "id": "816437322781949972",
                "name": "airhorn",
                "type": 1,
                "options": [
                    {"type": 3, "name": "variant", "value": "data a user is typ", "focused": true}
                ]
            },
            "context": 0
        })) else {
            panic!("expected an autocomplete interaction");
        };

        assert_eq!(autocomplete.user.id, UserId::new(500));
        assert_eq!(autocomplete.data.name, "airhorn");
        let focused = autocomplete.focused_option().expect("focused option");
        assert_eq!(focused.name, "variant");
        assert_eq!(focused.value.as_str(), Some("data a user is typ"));
        assert!(focused.focused);
    }

    #[test]
    fn the_focused_option_is_found_inside_subcommands() {
        let Interaction::Autocomplete(autocomplete) = interaction(json!({
            "type": 4,
            "id": "1",
            "application_id": "2",
            "token": "t",
            "user": {"id": "3", "username": "u"},
            "data": {
                "id": "4",
                "name": "cmd",
                "type": 1,
                "options": [{
                    "type": 1,
                    "name": "sub",
                    "options": [
                        {"type": 3, "name": "first", "value": "filled"},
                        {"type": 3, "name": "typing", "value": "partial", "focused": true}
                    ]
                }]
            }
        })) else {
            panic!("expected an autocomplete interaction");
        };

        let focused = autocomplete.focused_option().expect("focused option");
        assert_eq!(focused.name, "typing");
    }

    #[test]
    fn a_command_data_without_a_focused_option_returns_none() {
        let Interaction::Command(command) = interaction(command_payload()) else {
            panic!("expected a command interaction");
        };

        assert!(command.data.focused_option().is_none());
    }

    #[test]
    fn parses_a_modal_submit_interaction_with_nested_submitted_values() {
        let Interaction::ModalSubmit(modal) = interaction(json!({
            "type": 5,
            "id": "100",
            "application_id": "200",
            "token": "token",
            "guild_id": "300",
            "channel_id": "400",
            "member": {
                "user": {"id": "500", "username": "invoker", "discriminator": "0"},
                "nick": null,
                "roles": [],
                "joined_at": "2020-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false
            },
            "data": {
                "custom_id": "bug_modal",
                "components": [
                    {
                        "type": 18,
                        "id": 1,
                        "component": {
                            "type": 3,
                            "id": 2,
                            "custom_id": "favorite_bug",
                            "values": ["butterfly"]
                        }
                    },
                    {
                        "component": {
                            "custom_id": "game_feedback",
                            "id": 2,
                            "type": 4,
                            "value": "The acceleration feels better"
                        },
                        "id": 1,
                        "type": 18
                    }
                ]
            },
            "context": 0
        })) else {
            panic!("expected a modal submit interaction");
        };

        let selected = modal
            .data
            .component("favorite_bug")
            .expect("submitted string select");
        let Component::StringSelect(select) = selected else {
            panic!("expected a string select");
        };
        assert_eq!(select.values, vec!["butterfly".to_owned()]);

        let input = modal
            .data
            .component("game_feedback")
            .expect("submitted text input");
        let Component::TextInput(input) = input else {
            panic!("expected a text input");
        };
        assert_eq!(
            input.value.as_deref(),
            Some("The acceleration feels better")
        );

        assert_eq!(modal.data.custom_id, "bug_modal");
        assert!(modal.data.resolved.is_none());
        assert_eq!(modal.user.id, UserId::new(500));
    }

    #[test]
    fn modal_submit_data_resolves_select_entities() {
        let data: ModalSubmitData = serde_json::from_value(json!({
            "custom_id": "role_modal",
            "components": [],
            "resolved": {
                "roles": {
                    "222222222222222222": {
                        "id": "222222222222222222",
                        "name": "Developer",
                        "color": 12_745_742,
                        "hoist": false,
                        "position": 2,
                        "permissions": "0",
                        "managed": false,
                        "mentionable": true
                    }
                }
            }
        }))
        .expect("modal submit data");

        let resolved = data.resolved.expect("resolved data");
        assert_eq!(
            resolved
                .role(RoleId::new(222_222_222_222_222_222))
                .map(|role| role.name.as_str()),
            Some("Developer")
        );
    }

    #[test]
    fn resolved_channels_messages_and_attachments_are_typed() {
        let data: ComponentData = serde_json::from_value(json!({
            "custom_id": "channel_select",
            "component_type": 8,
            "values": ["333333333333333333"],
            "resolved": {
                "channels": {
                    "333333333333333333": {
                        "flags": 0,
                        "guild_id": "44444444444444444",
                        "id": "333333333333333333",
                        "last_message_id": null,
                        "name": "playtesting",
                        "nsfw": false,
                        "parent_id": "5555555555555555",
                        "permissions": "4503599627370495",
                        "position": 1,
                        "rate_limit_per_user": 0,
                        "topic": null,
                        "type": 0
                    }
                },
                "messages": {
                    "666666666666666666": {
                        "id": "666666666666666666",
                        "channel_id": "333333333333333333",
                        "author": {"id": "777777777777777777", "username": "target"},
                        "timestamp": "2026-10-22T17:00:00.000000+00:00",
                        "content": "a target message",
                        "pinned": false,
                        "type": 0
                    }
                },
                "attachments": {
                    "888888888888888888": {
                        "id": "888888888888888888",
                        "filename": "file.png",
                        "size": 10,
                        "url": "https://cdn.discordapp.com/attachments/1/888888888888888888/file.png"
                    }
                }
            }
        }))
        .expect("component data");

        let resolved = data.resolved.expect("resolved data");
        assert_eq!(
            resolved
                .channel(ChannelId::new(333_333_333_333_333_333))
                .and_then(|channel| channel.name.as_deref()),
            Some("playtesting")
        );
        assert_eq!(
            resolved
                .message(MessageId::new(666_666_666_666_666_666))
                .map(|message| message.content.as_str()),
            Some("a target message")
        );
        assert_eq!(
            resolved
                .attachment(AttachmentId::new(888_888_888_888_888_888))
                .map(|attachment| attachment.filename.as_str()),
            Some("file.png")
        );
    }

    #[test]
    fn malformed_resolved_entries_are_skipped_instead_of_failing() {
        let data: ComponentData = serde_json::from_value(json!({
            "custom_id": "user_select",
            "component_type": 5,
            "resolved": {
                "users": {
                    "1111111111111111111": {"id": "1111111111111111111", "username": "ok"},
                    "2222222222222222222": {"id": "not-a-snowflake"}
                }
            }
        }))
        .expect("component data");

        let resolved = data.resolved.expect("resolved data");
        assert_eq!(resolved.users.len(), 1);
        assert!(
            resolved
                .user(UserId::new(1_111_111_111_111_111_111))
                .is_some()
        );
    }

    #[test]
    fn an_unknown_integration_owner_key_is_ignored() {
        let owners: AuthorizingIntegrationOwners = serde_json::from_value(json!({
            "0": "300",
            "1": "500",
            "2": "999",
            "3": {"unexpected": "object"}
        }))
        .expect("authorizing integration owners");

        assert!(owners.guild_install.is_some());
        assert!(owners.user_install.is_some());
    }

    #[test]
    fn authorizing_integration_owners_serialise_back_to_the_documented_keys() {
        let owners = AuthorizingIntegrationOwners {
            guild_install: Some(IntegrationOwnerId::BotDm),
            user_install: Some(IntegrationOwnerId::Id(GenericId::new(500))),
        };

        let json = serde_json::to_value(&owners).expect("serialisable");

        assert_eq!(json, json!({"0": "0", "1": "500"}));
    }

    #[test]
    fn an_interaction_without_any_user_is_an_error() {
        let error = serde_json::from_value::<Interaction>(json!({
            "type": 2,
            "id": "1",
            "application_id": "2",
            "token": "t",
            "data": {"id": "3", "name": "n", "type": 1}
        }))
        .expect_err("no user and no member");

        assert!(error.to_string().contains("missing both"));
    }

    #[test]
    fn debug_redacts_every_interaction_token() {
        let ping = PingInteraction {
            id: InteractionId::new(1),
            application_id: ApplicationId::new(2),
            token: "ping-secret-token".to_owned(),
            version: 1,
        };
        let debug = format!("{ping:?}");
        assert!(debug.contains("PingInteraction"));
        assert!(!debug.contains("ping-secret-token"));

        let command = match interaction(command_payload()) {
            Interaction::Command(command) => command,
            other => panic!("expected a command interaction, got {other:?}"),
        };
        assert!(!format!("{command:?}").contains("interaction-token"));

        let component = match interaction(json!({
            "type": 3, "id": "1", "application_id": "2", "token": "component-secret",
            "user": {"id": "3", "username": "u"},
            "data": {"custom_id": "c", "component_type": 2}
        })) {
            Interaction::Component(component) => component,
            other => panic!("expected a component interaction, got {other:?}"),
        };
        assert!(!format!("{component:?}").contains("component-secret"));

        let autocomplete = match interaction(json!({
            "type": 4, "id": "1", "application_id": "2", "token": "autocomplete-secret",
            "user": {"id": "3", "username": "u"},
            "data": {"id": "4", "name": "n", "type": 1, "options": []}
        })) {
            Interaction::Autocomplete(autocomplete) => autocomplete,
            other => panic!("expected an autocomplete interaction, got {other:?}"),
        };
        assert!(!format!("{autocomplete:?}").contains("autocomplete-secret"));

        let modal = match interaction(json!({
            "type": 5, "id": "1", "application_id": "2", "token": "modal-secret",
            "user": {"id": "3", "username": "u"},
            "data": {"custom_id": "c", "components": []}
        })) {
            Interaction::ModalSubmit(modal) => modal,
            other => panic!("expected a modal submit interaction, got {other:?}"),
        };
        assert!(!format!("{modal:?}").contains("modal-secret"));
    }

    #[test]
    fn parses_every_command_option_kind() {
        let subcommand = option(json!({
            "name": "sub",
            "type": 1,
            "options": [
                {"name": "nested", "type": 3, "value": "value"}
            ]
        }));
        assert_eq!(subcommand.kind, CommandOptionKind::Subcommand);
        assert!(!subcommand.focused);
        let CommandOptionValue::Subcommand(nested) = &subcommand.value else {
            panic!("expected a subcommand value");
        };
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].value.as_str(), Some("value"));

        let group = option(json!({
            "name": "group",
            "type": 2,
            "options": [
                {"name": "sub", "type": 1, "options": []}
            ]
        }));
        assert_eq!(group.kind, CommandOptionKind::SubcommandGroup);
        let CommandOptionValue::SubcommandGroup(nested) = &group.value else {
            panic!("expected a subcommand group value");
        };
        assert_eq!(nested.len(), 1);

        let string = option(json!({"name": "s", "type": 3, "value": "hello"}));
        assert_eq!(string.value.as_str(), Some("hello"));

        let integer = option(json!({"name": "i", "type": 4, "value": -7}));
        assert_eq!(integer.value.as_i64(), Some(-7));
        assert_eq!(integer.value.as_f64(), Some(-7.0));

        let boolean = option(json!({"name": "b", "type": 5, "value": true}));
        assert_eq!(boolean.value.as_bool(), Some(true));

        let user = option(json!({"name": "u", "type": 6, "value": "123"}));
        assert_eq!(user.value.as_user_id(), Some(UserId::new(123)));

        let channel = option(json!({"name": "c", "type": 7, "value": "456"}));
        assert_eq!(channel.value.as_channel_id(), Some(ChannelId::new(456)));

        let role = option(json!({"name": "r", "type": 8, "value": "789"}));
        assert_eq!(role.value.as_role_id(), Some(RoleId::new(789)));

        let mentionable = option(json!({"name": "m", "type": 9, "value": "789"}));
        assert_eq!(
            mentionable.value,
            CommandOptionValue::Mentionable(GenericId::new(789))
        );

        let number = option(json!({"name": "n", "type": 10, "value": 1.5}));
        assert_eq!(number.value.as_f64(), Some(1.5));

        let integer_number = option(json!({"name": "n", "type": 10, "value": 2}));
        assert_eq!(integer_number.value.as_f64(), Some(2.0));

        let attachment = option(json!({"name": "a", "type": 11, "value": "999"}));
        assert_eq!(
            attachment.value,
            CommandOptionValue::Attachment(AttachmentId::new(999))
        );
    }

    #[test]
    fn malformed_option_values_are_errors() {
        // A string option without a value.
        assert!(
            serde_json::from_value::<CommandDataOption>(json!({"name": "s", "type": 3})).is_err()
        );

        // A user option whose value is not a snowflake.
        assert!(
            serde_json::from_value::<CommandDataOption>(
                json!({"name": "u", "type": 6, "value": "not-a-snowflake"})
            )
            .is_err()
        );

        // An unknown option type.
        assert!(
            serde_json::from_value::<CommandDataOption>(
                json!({"name": "x", "type": 42, "value": 1})
            )
            .is_err()
        );
    }

    #[test]
    fn command_option_kinds_round_trip_the_documented_integers() {
        for (value, kind) in [
            (1, CommandOptionKind::Subcommand),
            (2, CommandOptionKind::SubcommandGroup),
            (3, CommandOptionKind::Str),
            (4, CommandOptionKind::Integer),
            (5, CommandOptionKind::Boolean),
            (6, CommandOptionKind::User),
            (7, CommandOptionKind::Channel),
            (8, CommandOptionKind::Role),
            (9, CommandOptionKind::Mentionable),
            (10, CommandOptionKind::Number),
            (11, CommandOptionKind::Attachment),
        ] {
            assert_eq!(CommandOptionKind::from_i64(value), Some(kind));
            assert_eq!(kind.as_i64(), value);

            let json = serde_json::to_value(kind).expect("serialisable");
            assert_eq!(json, json!(value));

            let parsed: CommandOptionKind = serde_json::from_value(json).expect("deserialisable");
            assert_eq!(parsed, kind);
        }

        assert_eq!(CommandOptionKind::from_i64(0), None);
        assert_eq!(CommandOptionKind::from_i64(12), None);
    }

    #[test]
    fn command_kinds_round_trip_the_documented_integers() {
        for (value, kind) in [
            (1, CommandKind::ChatInput),
            (2, CommandKind::User),
            (3, CommandKind::Message),
            (4, CommandKind::PrimaryEntryPoint),
        ] {
            assert_eq!(CommandKind::from_i64(value), Some(kind));
            assert_eq!(kind.as_i64(), value);

            let parsed: CommandKind = serde_json::from_value(json!(value)).expect("deserialisable");
            assert_eq!(parsed, kind);
        }

        assert!(serde_json::from_value::<CommandKind>(json!(5)).is_err());
    }

    #[test]
    fn interaction_callback_types_cover_the_documented_table() {
        for (value, kind) in [
            (1, InteractionCallbackType::Pong),
            (4, InteractionCallbackType::ChannelMessageWithSource),
            (5, InteractionCallbackType::DeferredChannelMessageWithSource),
            (6, InteractionCallbackType::DeferredUpdateMessage),
            (7, InteractionCallbackType::UpdateMessage),
            (
                8,
                InteractionCallbackType::ApplicationCommandAutocompleteResult,
            ),
            (9, InteractionCallbackType::Modal),
            (10, InteractionCallbackType::PremiumRequired),
            (12, InteractionCallbackType::LaunchActivity),
        ] {
            assert_eq!(InteractionCallbackType::from_i64(value), kind);
            assert_eq!(kind.as_i64(), value);

            let json = serde_json::to_value(kind).expect("serialisable");
            assert_eq!(json, json!(value));
        }

        assert_eq!(
            InteractionCallbackType::from_i64(42),
            InteractionCallbackType::Unknown(42)
        );
    }

    #[test]
    fn interaction_contexts_cover_the_documented_table() {
        for (value, context) in [
            (0, InteractionContext::Guild),
            (1, InteractionContext::BotDm),
            (2, InteractionContext::PrivateChannel),
        ] {
            assert_eq!(InteractionContext::from_i64(value), context);
            assert_eq!(context.as_i64(), value);

            let json = serde_json::to_value(context).expect("serialisable");
            assert_eq!(json, json!(value));
        }
    }

    #[test]
    fn parses_a_registered_command() {
        let command: Command = serde_json::from_value(json!({
            "id": "100",
            "application_id": "200",
            "guild_id": "300",
            "name": "ban",
            "description": "Bans a user from the guild.",
            "options": [
                {
                    "type": 6,
                    "name": "target",
                    "description": "The user to ban.",
                    "required": true
                },
                {
                    "type": 3,
                    "name": "reason",
                    "description": "Why.",
                    "autocomplete": true
                },
                {
                    "type": 4,
                    "name": "duration",
                    "description": "How long to ban for.",
                    "required": false,
                    "min_value": 0,
                    "max_value": 7
                }
            ],
            "default_member_permissions": "4"
        }))
        .expect("registered command payload");

        assert_eq!(command.id, CommandId::new(100));
        assert_eq!(command.application_id, ApplicationId::new(200));
        assert_eq!(command.guild_id, Some(GuildId::new(300)));
        assert_eq!(command.name, "ban");
        assert_eq!(command.options.len(), 3);
        assert_eq!(command.options[0].kind, CommandOptionKind::User);
        assert_eq!(command.options[0].required, Some(true));
        assert_eq!(command.options[0].autocomplete, None);
        assert_eq!(command.options[1].autocomplete, Some(true));
        assert_eq!(command.options[2].kind, CommandOptionKind::Integer);
        assert_eq!(command.options[2].min_value, Some(json!(0)));
        assert_eq!(command.options[2].max_value, Some(json!(7)));
        assert_eq!(
            command.default_member_permissions,
            Some(Permissions::BAN_MEMBERS)
        );
    }

    #[test]
    fn registered_command_defaults_optional_fields() {
        let command: Command = serde_json::from_value(json!({
            "id": "100",
            "application_id": "200",
            "name": "ping",
            "description": "Pong!"
        }))
        .expect("minimal registered command");

        assert_eq!(command.guild_id, None);
        assert!(command.options.is_empty());
        assert_eq!(command.default_member_permissions, None);
    }

    #[test]
    fn option_accessors_return_none_for_other_kinds() {
        let value = CommandOptionValue::String("x".to_owned());

        assert_eq!(value.as_str(), Some("x"));
        assert_eq!(value.as_i64(), None);
        assert_eq!(value.as_bool(), None);
        assert_eq!(value.as_f64(), None);
        assert_eq!(value.as_user_id(), None);
        assert_eq!(value.as_channel_id(), None);
        assert_eq!(value.as_role_id(), None);
    }

    #[test]
    fn parses_an_interaction_callback_response_with_a_created_message() {
        let response: InteractionCallbackResponse = serde_json::from_value(json!({
            "interaction": {
                "id": "100",
                "type": 2,
                "response_message_id": "1000",
                "response_message_loading": false,
                "response_message_ephemeral": false
            },
            "resource": {
                "type": 4,
                "message": {
                    "id": "1000",
                    "channel_id": "400",
                    "author": {"id": "200", "username": "bot"},
                    "timestamp": "2026-10-22T17:00:00.000000+00:00",
                    "content": "Pong!",
                    "pinned": false,
                    "type": 20
                }
            }
        }))
        .expect("callback response");

        assert_eq!(response.interaction.id, InteractionId::new(100));
        assert_eq!(
            response.interaction.kind,
            InteractionType::ApplicationCommand
        );
        assert_eq!(
            response.interaction.response_message_id,
            Some(MessageId::new(1000))
        );
        let message = response.created_message().expect("created message");
        assert_eq!(message.content, "Pong!");
        let resource = response.resource.expect("resource");
        assert_eq!(
            resource.kind,
            InteractionCallbackType::ChannelMessageWithSource
        );
    }

    #[test]
    fn an_interaction_callback_response_without_a_resource_has_no_message() {
        let response: InteractionCallbackResponse = serde_json::from_value(json!({
            "interaction": {"id": "1", "type": 5}
        }))
        .expect("callback response");

        assert!(response.created_message().is_none());
        assert!(response.resource.is_none());
    }

    #[test]
    fn parses_a_message_interaction_object() {
        let interaction: MessageInteraction = serde_json::from_value(json!({
            "id": "100",
            "type": 2,
            "name": "deploy",
            "user": {"id": "500", "username": "invoker"},
            "member": {"nick": "Invoker", "roles": ["600"]}
        }))
        .expect("message interaction");

        assert_eq!(interaction.id, InteractionId::new(100));
        assert_eq!(interaction.kind, InteractionType::ApplicationCommand);
        assert_eq!(interaction.name, "deploy");
        assert_eq!(interaction.user.id, UserId::new(500));
        assert_eq!(
            interaction.member.as_ref().and_then(|m| m.nick.as_deref()),
            Some("Invoker")
        );
    }
}
