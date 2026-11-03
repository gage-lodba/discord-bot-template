//! Message and embed models.
//!
//! Reference: `docs/message.md` (Message Object, Embed Object, Attachment
//! Object) and `docs/gateway-events.md` (Message Create Extra Fields). Polls
//! are documented in `docs/poll.md` (Poll Object and its nested media, answer
//! and results objects).

use serde::{Deserialize, Deserializer, Serialize};

use crate::id::{AttachmentId, ChannelId, GuildId, MessageId};
use crate::model::application::MessageInteraction;
use crate::model::component::Component;
use crate::model::emoji::ReactionType;
use crate::model::guild::Member;
use crate::model::user::User;
use crate::timestamp::Timestamp;

/// The documented message flags bitfield.
///
/// Reference: `docs/message.md` (Message Flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageFlags(pub u64);

impl MessageFlags {
    /// `CROSSPOSTED` (`1 << 0`): published to subscribed channels.
    pub const CROSSPOSTED: Self = Self(1 << 0);
    /// `IS_CROSSPOST` (`1 << 1`): originated from another channel.
    pub const IS_CROSSPOST: Self = Self(1 << 1);
    /// `SUPPRESS_EMBEDS` (`1 << 2`): do not serialise embeds.
    pub const SUPPRESS_EMBEDS: Self = Self(1 << 2);
    /// `SOURCE_MESSAGE_DELETED` (`1 << 3`): the crosspost source was deleted.
    pub const SOURCE_MESSAGE_DELETED: Self = Self(1 << 3);
    /// `URGENT` (`1 << 4`): sent by the urgent message system.
    pub const URGENT: Self = Self(1 << 4);
    /// `HAS_THREAD` (`1 << 5`): the message has a thread.
    pub const HAS_THREAD: Self = Self(1 << 5);
    /// `EPHEMERAL` (`1 << 6`): visible only to the interaction's invoker.
    pub const EPHEMERAL: Self = Self(1 << 6);
    /// `LOADING` (`1 << 7`): an interaction response still "thinking".
    pub const LOADING: Self = Self(1 << 7);
    /// `FAILED_TO_MENTION_SOME_ROLES_IN_THREAD` (`1 << 8`).
    pub const FAILED_TO_MENTION_SOME_ROLES_IN_THREAD: Self = Self(1 << 8);
    /// `SUPPRESS_NOTIFICATIONS` (`1 << 12`): no push/desktop notification.
    pub const SUPPRESS_NOTIFICATIONS: Self = Self(1 << 12);
    /// `IS_VOICE_MESSAGE` (`1 << 13`): the message is a voice message.
    pub const IS_VOICE_MESSAGE: Self = Self(1 << 13);
    /// `HAS_SNAPSHOT` (`1 << 14`): the message has a forwarded snapshot.
    pub const HAS_SNAPSHOT: Self = Self(1 << 14);
    /// `IS_COMPONENTS_V2` (`1 << 15`): the message is component-driven.
    pub const IS_COMPONENTS_V2: Self = Self(1 << 15);

    /// Returns an empty flag set.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns true when no flag is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns true when every flag in `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Sets `other` and returns whether any of its bits were newly set.
    pub const fn insert(&mut self, other: Self) -> bool {
        let was_set = self.contains(other);
        self.0 |= other.0;
        !was_set
    }

    /// Clears `other` and returns whether any of its bits were set.
    pub const fn remove(&mut self, other: Self) -> bool {
        let was_set = self.contains(other);
        self.0 &= !other.0;
        was_set
    }
}

impl From<u64> for MessageFlags {
    fn from(bits: u64) -> Self {
        Self(bits)
    }
}

impl From<MessageFlags> for u64 {
    fn from(flags: MessageFlags) -> Self {
        flags.0
    }
}

impl std::ops::BitOr for MessageFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for MessageFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl std::ops::BitAnd for MessageFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl std::ops::BitAndAssign for MessageFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl std::ops::Not for MessageFlags {
    type Output = Self;

    fn not(self) -> Self {
        Self(!self.0)
    }
}

/// A message sent in a channel.
///
/// Only the fields this crate consumes are modelled. `content` is empty when
/// the bot has not been approved for the `MESSAGE_CONTENT` privileged intent
/// (`docs/message.md`, Message Structure). The `member` field is deserialised
/// leniently because `MESSAGE_CREATE`/`MESSAGE_UPDATE` member objects omit the
/// nested `user` field (`docs/gateway-events.md`, Message Create).
///
/// `components` accepts every documented component type: an unmodelled one is
/// kept as [`crate::model::component::Component::Other`] rather than failing
/// the message (`docs/components-reference.md`, Component Types).
///
/// Reference: `docs/message.md` (Message Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// The id of the message.
    pub id: MessageId,
    /// The id of the channel the message was sent in.
    pub channel_id: ChannelId,
    /// The id of the guild the message was sent in, when it is a guild message.
    #[serde(default)]
    pub guild_id: Option<GuildId>,
    /// The author of the message.
    pub author: User,
    /// The contents of the message.
    #[serde(default)]
    pub content: String,
    /// Whether the message is text-to-speech.
    ///
    /// The field is optional on the wire. It is serialised explicitly even when
    /// false rather than skipped; the documented message object accepts either
    /// form, and serialising it avoids a serde `&bool` predicate.
    #[serde(default)]
    pub tts: bool,
    /// When the message was sent.
    pub timestamp: Timestamp,
    /// When the message was edited, or `None` if it was never edited.
    #[serde(default)]
    pub edited_timestamp: Option<Timestamp>,
    /// The author's guild member data, present on guild messages.
    ///
    /// The member object attached to message events has no nested `user`, so a
    /// member without one deserialises to `None` instead of failing the whole
    /// message.
    #[serde(default, deserialize_with = "deserialize_optional_member")]
    pub member: Option<Member>,
    /// Any embedded content.
    #[serde(default)]
    pub embeds: Vec<Embed>,
    /// The components attached to this message.
    ///
    /// Reference: `docs/components-reference.md` (Component Object).
    #[serde(default)]
    pub components: Vec<Component>,
    /// The files attached to this message.
    ///
    /// Reference: `docs/message.md` (Attachment Object).
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// The poll attached to this message, when there is one.
    ///
    /// The reference notes that a poll is omitted entirely when the app has not
    /// configured the `MESSAGE_CONTENT` privileged intent, and that a poll with
    /// no `results` object means "unknown results" rather than "no votes"
    /// (`docs/poll.md`, Poll Results Object), so both levels stay optional.
    ///
    /// Reference: `docs/message.md` (Message Structure, `poll`) and
    /// `docs/poll.md` (Poll Object).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_poll"
    )]
    pub poll: Option<Poll>,
    /// The interaction this message is a response to, when there is one.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Message Interaction
    /// Object).
    #[serde(default)]
    pub interaction: Option<MessageInteraction>,
    /// Whether the message is pinned.
    #[serde(default)]
    pub pinned: bool,
    /// The message flags bitfield.
    ///
    /// The field is optional on the wire. It is serialised explicitly even when
    /// the set is empty rather than skipped; the documented message object
    /// accepts either form.
    ///
    /// Reference: `docs/message.md` (Message Flags).
    #[serde(default)]
    pub flags: MessageFlags,
    /// The message type, `type` on the wire.
    ///
    /// Reference: `docs/message.md` (Message Types).
    #[serde(rename = "type", default)]
    pub kind: Option<i64>,
}

impl Message {
    /// Whether this message carries the documented `EPHEMERAL` flag
    /// (`docs/message.md`, Message Flags).
    #[must_use]
    pub const fn is_ephemeral(&self) -> bool {
        self.flags.contains(MessageFlags::EPHEMERAL)
    }

    /// Whether this message carries the documented `IS_COMPONENTS_V2` flag
    /// (`docs/message.md`, Message Flags).
    #[must_use]
    pub const fn is_components_v2(&self) -> bool {
        self.flags.contains(MessageFlags::IS_COMPONENTS_V2)
    }
}

/// A file attached to a message.
///
/// Reference: `docs/message.md` (Attachment Object, Attachment Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// The attachment id.
    pub id: AttachmentId,
    /// The name of the attached file.
    pub filename: String,
    /// The title of the file, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The description (alt text) of the file, at most 1024 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The attachment's media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// The size of the file in bytes.
    pub size: u64,
    /// The source url of the file.
    pub url: String,
    /// A proxied url of the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// The height of the file, if it is an image or video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// The width of the file, if it is an image or video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// The thumbhash placeholder of the file, if it is an image or video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The version of the placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_version: Option<u32>,
    /// Whether the attachment is ephemeral.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    /// The duration of an audio or video file, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    /// A base64 encoded sampled waveform, for voice messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waveform: Option<String>,
    /// The attachment flags bitfield.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// The question or one answer of a [`Poll`].
///
/// Only `text` is supported for a poll question; answers may also carry an
/// optional partial emoji. The reference says `text` should always be present
/// but asks clients not to depend on it, so it is optional here.
///
/// Reference: `docs/poll.md` (Poll Media Object).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollMedia {
    /// The text of the field, at most 300 characters for the question and 55
    /// for an answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The partial emoji of the field; only answers may have one.
    ///
    /// A malformed emoji object is treated as absent instead of failing the
    /// whole poll, because the reference marks the field optional and asks
    /// clients not to rely on its shape.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_emoji"
    )]
    pub emoji: Option<ReactionType>,
}

/// One answer of a [`Poll`].
///
/// Reference: `docs/poll.md` (Poll Answer Object).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollAnswer {
    /// The id that labels this answer.
    ///
    /// The reference only sends `answer_id` in responses from Discord's
    /// API/Gateway (it is not part of the create request), so it defaults to
    /// `0` rather than failing a lenient parse.
    #[serde(default)]
    pub answer_id: u32,
    /// The text and optional emoji of the answer.
    #[serde(default)]
    pub poll_media: PollMedia,
}

/// The results of a [`Poll`].
///
/// The whole object is absent in responses where the backend did not fetch the
/// results; that means "unknown results", not "no votes" (`docs/poll.md`, Poll
/// Results Object).
///
/// Reference: `docs/poll.md` (Poll Results Object and Poll Answer Count
/// Object).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollResults {
    /// Whether the votes have been precisely counted by the final tally.
    #[serde(default)]
    pub is_finalized: bool,
    /// The counts for each answer; an answer that is missing has no votes.
    #[serde(default)]
    pub answer_counts: Vec<PollAnswerCount>,
}

/// The number of votes for one [`PollAnswer`].
///
/// Reference: `docs/poll.md` (Poll Answer Count Object).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollAnswerCount {
    /// The [`PollAnswer::answer_id`] this count belongs to.
    #[serde(default)]
    pub id: u32,
    /// The number of votes for the answer.
    #[serde(default)]
    pub count: u32,
    /// Whether the current user voted for this answer.
    #[serde(default)]
    pub me_voted: bool,
}

/// The layout of a [`Poll`].
///
/// The reference documents only `DEFAULT` (`1`) and reserves the other values
/// for future layouts, so an unknown value is preserved instead of failing the
/// poll. This is the same lenient shape the crate uses for other open enums.
///
/// Reference: `docs/poll.md` (Layout Type).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum PollLayout {
    /// `DEFAULT` (`1`): the default layout.
    #[default]
    Default,
    /// A layout this crate does not model, preserved numerically.
    Unknown(i64),
}

impl PollLayout {
    /// Returns the documented integer id of the layout.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Default => 1,
            Self::Unknown(id) => id,
        }
    }
}

impl Serialize for PollLayout {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for PollLayout {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = i64::deserialize(deserializer)?;

        Ok(match id {
            1 => Self::Default,
            other => Self::Unknown(other),
        })
    }
}

/// A poll attached to a message.
///
/// Only fields this crate consumes are modelled; every object is lenient so a
/// future field or layout never fails a message event.
///
/// Reference: `docs/poll.md` (Poll Object).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poll {
    /// The poll question. Only `text` is supported by Discord.
    #[serde(default)]
    pub question: PollMedia,
    /// The answers available in the poll; currently at most 10.
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    /// When the poll ends. Currently always present on inbound polls, but the
    /// reference marks it nullable for future non-expiring polls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiry: Option<Timestamp>,
    /// Whether a user can select multiple answers.
    #[serde(default)]
    pub allow_multiselect: bool,
    /// The layout of the poll; currently always [`PollLayout::Default`].
    #[serde(default)]
    pub layout_type: PollLayout,
    /// The results, absent when Discord did not fetch them (unknown results).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<PollResults>,
}

/// Deserialises the optional `poll` field of a message leniently.
///
/// A poll whose shape this model does not understand is treated as absent
/// rather than failing the whole message: a malformed optional object must
/// never drop a `MESSAGE_CREATE` event.
fn deserialize_optional_poll<'de, D>(deserializer: D) -> Result<Option<Poll>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;

    Ok(value.and_then(|value| serde_json::from_value::<Poll>(value).ok()))
}

/// Deserialises an optional partial emoji leniently.
///
/// [`ReactionType`] cannot represent an empty emoji object, but the poll media
/// object marks `emoji` optional and extensible, so an unrepresentable value is
/// treated as absent instead of failing the message.
fn deserialize_optional_emoji<'de, D>(deserializer: D) -> Result<Option<ReactionType>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;

    Ok(value.and_then(|value| serde_json::from_value::<ReactionType>(value).ok()))
}

/// The content of a rich embed.
///
/// The same type is used for inbound messages and outbound payloads: it
/// serialises with `type` as the wire key, omits unset optional fields and
/// omits an empty `fields` array, so a builder can construct it directly.
///
/// Reference: `docs/message.md` (Embed Object, Embed Limits).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Embed {
    /// The title of the embed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The type of embed, `type` on the wire (always `"rich"` for webhook
    /// embeds).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The description of the embed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The url of the embed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The timestamp of the embed content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<Timestamp>,
    /// The colour code of the embed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    /// The footer information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<EmbedFooter>,
    /// The image information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<EmbedImage>,
    /// The thumbnail information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<EmbedImage>,
    /// The video information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<EmbedVideo>,
    /// The provider information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<EmbedProvider>,
    /// The author information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<EmbedAuthor>,
    /// The fields of the embed, at most 25.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<EmbedField>,
}

/// The footer of an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Footer Structure).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedFooter {
    /// The footer text, at most 2048 characters.
    pub text: String,
    /// The url of the footer icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// A proxied url of the footer icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_icon_url: Option<String>,
}

/// An image or thumbnail inside an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Image Structure).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedImage {
    /// The source url of the image.
    pub url: String,
    /// A proxied url of the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// The height of the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// The width of the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// The image's media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// A placeholder for the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The version of the placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_version: Option<u32>,
    /// The description (alt text) of the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The embed media flags of the image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
}

/// A video inside an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Video Structure).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedVideo {
    /// The source url of the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// A proxied url of the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// The height of the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// The width of the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// The video's media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// A placeholder for the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The version of the placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_version: Option<u32>,
    /// The description (alt text) of the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The embed media flags of the video.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u32>,
}

/// The provider of an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Provider Structure).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedProvider {
    /// The name of the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The url of the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// The author of an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Author Structure).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedAuthor {
    /// The name of the author, at most 256 characters.
    pub name: String,
    /// The url of the author.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The url of the author icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// A proxied url of the author icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_icon_url: Option<String>,
}

/// One field of an [`Embed`].
///
/// `inline` is optional in `docs/message.md` (Embed Field Structure) and is
/// omitted from outbound JSON when `None`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedField {
    /// The name of the field, at most 256 characters.
    pub name: String,
    /// The value of the field, at most 1024 characters.
    pub value: String,
    /// Whether the field should display inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline: Option<bool>,
}

/// Deserialises the `member` field of a message leniently.
///
/// `docs/gateway-events.md` (Message Create) says the member object attached to
/// message events does not include the nested `user` object, so a member
/// without a user is treated as absent instead of failing the message. Any
/// other malformed member is also treated as absent: a message is still useful
/// without its member metadata.
fn deserialize_optional_member<'de, D>(deserializer: D) -> Result<Option<Member>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;

    Ok(value.and_then(|value| serde_json::from_value::<Member>(value).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::EmojiId;

    #[test]
    fn parses_the_documented_message() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "reactions": [
                {
                    "count": 1,
                    "count_details": {"burst": 0, "normal": 1},
                    "me": false,
                    "me_burst": false,
                    "emoji": {"id": null, "name": "🔥"},
                    "burst_colors": []
                }
            ],
            "attachments": [],
            "tts": false,
            "embeds": [],
            "timestamp": "2017-07-11T17:27:07.299000+00:00",
            "mention_everyone": false,
            "id": "334385199974967042",
            "pinned": false,
            "edited_timestamp": null,
            "author": {
                "username": "Mason",
                "discriminator": "9999",
                "id": "53908099506183680",
                "avatar": "a_bab14f271d565501444b2ca3be944b25"
            },
            "mention_roles": [],
            "content": "Supa Hot",
            "channel_id": "290926798999357250",
            "mentions": [],
            "type": 0
        }))
        .expect("documented message payload");

        assert_eq!(message.id, MessageId::new(334_385_199_974_967_042));
        assert_eq!(message.channel_id, ChannelId::new(290_926_798_999_357_250));
        assert_eq!(message.guild_id, None);
        assert_eq!(message.author.username, "Mason");
        assert_eq!(message.content, "Supa Hot");
        assert_eq!(
            message.timestamp,
            Timestamp::from_unix_millis(1_499_794_027_299)
        );
        assert_eq!(message.edited_timestamp, None);
        assert_eq!(message.member, None);
        assert!(message.embeds.is_empty());
        assert!(!message.pinned);
        assert_eq!(message.kind, Some(0));
    }

    #[test]
    fn message_without_content_defaults_it_to_empty() {
        // An app without MESSAGE_CONTENT gets empty content and embeds.
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 0
        }))
        .expect("message payload");

        assert_eq!(message.content, "");
        assert!(message.embeds.is_empty());
    }

    #[test]
    fn message_member_without_a_user_is_treated_as_absent() {
        // docs/gateway-events.md: the member object attached to MESSAGE_CREATE
        // does not include the nested user object.
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "guild_id": "3",
            "author": {"id": "4", "username": "member"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "content": "hi",
            "pinned": false,
            "type": 0,
            "member": {
                "nick": "Nick",
                "roles": ["5"],
                "joined_at": "2020-01-01T00:00:00+00:00",
                "deaf": false,
                "mute": false,
                "pending": false
            }
        }))
        .expect("message payload");

        assert_eq!(message.guild_id, Some(GuildId::new(3)));
        assert_eq!(message.member, None);
    }

    #[test]
    fn message_member_with_a_user_parses() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "4", "username": "member"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 19,
            "member": {
                "user": {"id": "4", "username": "member", "discriminator": "0"},
                "nick": "Nick",
                "roles": ["5"],
                "joined_at": "2020-01-01T00:00:00+00:00",
                "deaf": false,
                "mute": false
            }
        }))
        .expect("message payload");

        let member = message.member.expect("member should parse");
        assert_eq!(member.display_name(), "Nick");
    }

    #[test]
    fn missing_required_message_fields_are_serde_errors() {
        assert!(
            serde_json::from_value::<Message>(serde_json::json!({
                "id": "1",
                "channel_id": "2",
                "author": {"id": "3", "username": "bot"}
                // timestamp missing
            }))
            .is_err()
        );
    }

    #[test]
    fn parses_the_documented_embed_shape() {
        let embed: Embed = serde_json::from_value(serde_json::json!({
            "title": "Some title",
            "type": "rich",
            "description": "Some description",
            "url": "https://example.com",
            "timestamp": "2023-01-01T00:00:00.000000+00:00",
            "color": 16_711_680,
            "footer": {
                "text": "footer text",
                "icon_url": "https://example.com/footer.png",
                "proxy_icon_url": "https://example.com/proxy.png"
            },
            "image": {
                "url": "https://example.com/image.png",
                "proxy_url": "https://example.com/image-proxy.png",
                "height": 100,
                "width": 200
            },
            "thumbnail": {
                "url": "https://example.com/thumb.png"
            },
            "video": {
                "url": "https://example.com/video.mp4",
                "height": 720,
                "width": 1280
            },
            "provider": {
                "name": "Example",
                "url": "https://example.com"
            },
            "author": {
                "name": "author name",
                "url": "https://example.com/author",
                "icon_url": "https://example.com/author.png"
            },
            "fields": [
                {"name": "field one", "value": "value one", "inline": true},
                {"name": "field two", "value": "value two"},
                {"name": "field three", "value": "value three", "inline": false}
            ],
            "future_field": "ignored"
        }))
        .expect("documented embed payload");

        assert_eq!(embed.title.as_deref(), Some("Some title"));
        assert_eq!(embed.kind.as_deref(), Some("rich"));
        assert_eq!(embed.description.as_deref(), Some("Some description"));
        assert_eq!(embed.url.as_deref(), Some("https://example.com"));
        assert_eq!(
            embed.timestamp,
            Some(Timestamp::from_unix_millis(1_672_531_200_000))
        );
        assert_eq!(embed.color, Some(16_711_680));
        assert_eq!(
            embed.footer.as_ref().map(|footer| footer.text.as_str()),
            Some("footer text")
        );
        assert_eq!(
            embed.image.as_ref().map(|image| image.url.as_str()),
            Some("https://example.com/image.png")
        );
        assert_eq!(
            embed.image.as_ref().and_then(|image| image.height),
            Some(100)
        );
        assert_eq!(
            embed.thumbnail.as_ref().map(|image| image.url.as_str()),
            Some("https://example.com/thumb.png")
        );
        assert_eq!(
            embed.video.as_ref().and_then(|video| video.width),
            Some(1280)
        );
        assert_eq!(
            embed
                .provider
                .as_ref()
                .and_then(|provider| provider.name.as_deref()),
            Some("Example")
        );
        assert_eq!(
            embed.author.as_ref().map(|author| author.name.as_str()),
            Some("author name")
        );
        assert_eq!(embed.fields.len(), 3);
        assert_eq!(embed.fields[0].inline, Some(true));
        assert_eq!(embed.fields[1].inline, None);
        assert_eq!(embed.fields[2].inline, Some(false));
    }

    #[test]
    fn embed_serialises_only_the_fields_that_are_set() {
        // The shape docs/message.md (Create Message) documents for outbound
        // payloads.
        let embed = Embed {
            title: Some("Hello, Embed!".to_owned()),
            description: Some("This is an embedded message.".to_owned()),
            ..Embed::default()
        };

        let json = serde_json::to_value(&embed).expect("serialisable");

        assert_eq!(
            json,
            serde_json::json!({
                "title": "Hello, Embed!",
                "description": "This is an embedded message."
            })
        );
    }

    #[test]
    fn embed_serialises_type_and_fields_in_the_wire_shape() {
        let embed = Embed {
            title: Some("title".to_owned()),
            kind: Some("rich".to_owned()),
            fields: vec![
                EmbedField {
                    name: "name".to_owned(),
                    value: "value".to_owned(),
                    inline: Some(true),
                },
                EmbedField {
                    name: "other".to_owned(),
                    value: "value".to_owned(),
                    inline: None,
                },
            ],
            ..Embed::default()
        };

        let json = serde_json::to_value(&embed).expect("serialisable");

        assert_eq!(json["type"], serde_json::json!("rich"));
        assert_eq!(
            json["fields"],
            serde_json::json!([
                {"name": "name", "value": "value", "inline": true},
                {"name": "other", "value": "value"}
            ])
        );
        assert_eq!(json.get("footer"), None);
        assert_eq!(json.get("color"), None);
    }

    #[test]
    fn empty_embed_serialises_to_an_empty_object() {
        let json = serde_json::to_value(Embed::default()).expect("serialisable");

        assert_eq!(json, serde_json::json!({}));
    }

    #[test]
    fn embed_substructures_default_missing_optional_fields() {
        let footer: EmbedFooter =
            serde_json::from_value(serde_json::json!({"text": "hi"})).expect("footer");
        assert_eq!(footer.text, "hi");
        assert_eq!(footer.icon_url, None);

        let author: EmbedAuthor =
            serde_json::from_value(serde_json::json!({"name": "me"})).expect("author");
        assert_eq!(author.name, "me");
        assert_eq!(author.url, None);

        let field: EmbedField =
            serde_json::from_value(serde_json::json!({"name": "n", "value": "v"})).expect("field");
        assert_eq!(field.inline, None);
    }

    #[test]
    fn parses_a_message_with_components_and_a_message_interaction() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 20,
            "components": [
                {
                    "type": 1,
                    "components": [
                        {"type": 2, "style": 1, "label": "Confirm", "custom_id": "confirm"}
                    ]
                }
            ],
            "interaction": {
                "id": "100",
                "type": 2,
                "name": "deploy",
                "user": {"id": "500", "username": "invoker"},
                "member": {"nick": "Invoker", "roles": [], "joined_at": null}
            }
        }))
        .expect("message with components");

        assert_eq!(message.components.len(), 1);
        assert_eq!(
            message.components[0].kind(),
            crate::model::component::ComponentKind::ActionRow
        );
        assert_eq!(message.components[0].custom_id(), None);

        let interaction = message.interaction.expect("message interaction");
        assert_eq!(interaction.id, crate::id::InteractionId::new(100));
        assert_eq!(
            interaction.kind,
            crate::model::application::InteractionType::ApplicationCommand
        );
        assert_eq!(interaction.name, "deploy");
        assert_eq!(interaction.user.id, crate::id::UserId::new(500));
        assert_eq!(
            interaction
                .member
                .as_ref()
                .and_then(|member| member.nick.as_deref()),
            Some("Invoker")
        );
    }

    #[test]
    fn unknown_component_types_do_not_fail_a_message() {
        // Layout/content component types are modelled elsewhere; this message
        // model must accept them (and any genuinely unknown type) without
        // failing, whatever typed variant ends up representing them.
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 20,
            "components": [
                {"type": 10, "content": "This is a Text Display component"},
                {"type": 17, "components": []},
                {"type": 99, "future": true}
            ]
        }))
        .expect("message with v2 components");

        assert_eq!(message.components.len(), 3);
        assert_eq!(message.components[0].kind().as_i64(), 10);
        assert_eq!(message.components[1].kind().as_i64(), 17);
        assert_eq!(message.components[2].kind().as_i64(), 99);
        assert!(message.interaction.is_none());
    }

    #[test]
    fn parses_attachments() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 0,
            "attachments": [{
                "id": "44",
                "filename": "screenshot.png",
                "title": "A screenshot",
                "description": "The dashboard",
                "content_type": "image/png",
                "size": 1_024,
                "url": "https://cdn.discordapp.com/attachments/1/44/screenshot.png",
                "proxy_url": "https://media.discordapp.net/attachments/1/44/screenshot.png",
                "height": 600,
                "width": 800,
                "ephemeral": false,
                "flags": 8
            }]
        }))
        .expect("message with attachments");

        assert_eq!(message.attachments.len(), 1);
        let attachment = &message.attachments[0];
        assert_eq!(attachment.id, AttachmentId::new(44));
        assert_eq!(attachment.filename, "screenshot.png");
        assert_eq!(attachment.content_type.as_deref(), Some("image/png"));
        assert_eq!(attachment.size, 1_024);
        assert_eq!(attachment.width, Some(800));
        assert_eq!(attachment.flags, Some(8));
    }

    #[test]
    fn a_message_without_components_has_an_empty_component_list() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 0
        }))
        .expect("minimal message");

        assert!(message.components.is_empty());
        assert!(message.attachments.is_empty());
        assert!(message.poll.is_none());
        assert!(message.interaction.is_none());
    }

    #[test]
    fn parses_the_documented_poll_object() {
        // The Poll Object and its nested media/answer/results objects, as
        // documented in docs/poll.md.
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 0,
            "poll": {
                "question": {"text": "What should we play tonight?"},
                "answers": [
                    {
                        "answer_id": 1,
                        "poll_media": {"text": "Minecraft", "emoji": {"name": "🟩"}}
                    },
                    {
                        "answer_id": 2,
                        "poll_media": {
                            "text": "Blurple",
                            "emoji": {
                                "id": "41771983429993937",
                                "name": "blurple",
                                "animated": false
                            }
                        }
                    }
                ],
                "expiry": "2024-01-02T03:04:05.000000+00:00",
                "allow_multiselect": true,
                "layout_type": 1,
                "results": {
                    "is_finalized": false,
                    "answer_counts": [
                        {"id": 1, "count": 3, "me_voted": true},
                        {"id": 2, "count": 0, "me_voted": false}
                    ]
                }
            }
        }))
        .expect("message with poll");

        let poll = message.poll.expect("the poll must parse");
        assert_eq!(
            poll.question.text.as_deref(),
            Some("What should we play tonight?")
        );
        assert_eq!(poll.question.emoji, None);
        assert_eq!(poll.answers.len(), 2);
        assert_eq!(poll.answers[0].answer_id, 1);
        assert_eq!(
            poll.answers[0].poll_media.text.as_deref(),
            Some("Minecraft")
        );
        assert_eq!(
            poll.answers[0].poll_media.emoji,
            Some(ReactionType::Unicode("🟩".to_owned()))
        );
        assert_eq!(
            poll.answers[1].poll_media.emoji,
            Some(ReactionType::Custom {
                id: EmojiId::new(41_771_983_429_993_937),
                name: Some("blurple".to_owned()),
                animated: false,
            })
        );
        assert_eq!(
            poll.expiry,
            Some(Timestamp::from_unix_millis(1_704_164_645_000))
        );
        assert!(poll.allow_multiselect);
        assert_eq!(poll.layout_type, PollLayout::Default);
        assert_eq!(poll.layout_type.as_i64(), 1);

        let results = poll.results.expect("the results must parse");
        assert!(!results.is_finalized);
        assert_eq!(results.answer_counts.len(), 2);
        assert_eq!(results.answer_counts[0].id, 1);
        assert_eq!(results.answer_counts[0].count, 3);
        assert!(results.answer_counts[0].me_voted);
        assert!(!results.answer_counts[1].me_voted);
    }

    #[test]
    fn poll_results_and_answer_emoji_are_optional() {
        // docs/poll.md: `results` may be absent (unknown results) and
        // `poll_media.emoji` is optional; neither may fail the message.
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 0,
            "poll": {
                "question": {"text": "Pick one"},
                "answers": [
                    {"answer_id": 1, "poll_media": {"text": "One"}},
                    {"answer_id": 2, "poll_media": {"text": "Two"}}
                ],
                "expiry": "2024-01-02T00:00:00+00:00",
                "allow_multiselect": false,
                "layout_type": 1
            }
        }))
        .expect("message with poll without results");

        let poll = message.poll.expect("the poll must parse");
        assert!(poll.results.is_none(), "absent results are unknown results");
        assert_eq!(poll.answers[0].poll_media.emoji, None);
        assert!(!poll.allow_multiselect);
    }

    #[test]
    fn unknown_poll_layout_values_are_preserved() {
        let poll: Poll = serde_json::from_value(serde_json::json!({
            "question": {"text": "q"},
            "answers": [],
            "layout_type": 7
        }))
        .expect("poll with a future layout");

        assert_eq!(poll.layout_type, PollLayout::Unknown(7));
        assert_eq!(poll.layout_type.as_i64(), 7);
        assert_eq!(
            serde_json::to_value(PollLayout::Default).expect("serialisable"),
            serde_json::json!(1)
        );
        assert_eq!(
            serde_json::to_value(PollLayout::Unknown(7)).expect("serialisable"),
            serde_json::json!(7)
        );
    }

    #[test]
    fn a_malformed_poll_is_treated_as_absent() {
        // A poll is optional data: an object this model cannot represent must
        // not drop the whole MESSAGE_CREATE event.
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "2",
            "author": {"id": "3", "username": "bot"},
            "timestamp": "2023-01-01T00:00:00+00:00",
            "pinned": false,
            "type": 0,
            "poll": {"question": 42}
        }))
        .expect("message with a malformed poll");

        assert!(message.poll.is_none());
    }

    #[test]
    fn poll_media_ignores_an_unrepresentable_emoji() {
        let media: PollMedia =
            serde_json::from_value(serde_json::json!({"text": "One", "emoji": {}}))
                .expect("media with an empty emoji object");

        assert_eq!(media.text.as_deref(), Some("One"));
        assert_eq!(media.emoji, None);
    }

    #[test]
    fn a_message_with_a_poll_and_attachments_round_trips_through_serde() {
        // The documented inbound fixture must deserialise, serialise and
        // deserialise again without losing the poll or attachment data.
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "334385199974967042",
            "channel_id": "290926798999357250",
            "guild_id": "290926798999357251",
            "author": {"id": "53908099506183680", "username": "Mason"},
            "content": "Vote!",
            "timestamp": "2017-07-11T17:27:07.299000+00:00",
            "pinned": false,
            "type": 0,
            "attachments": [{
                "id": "44",
                "filename": "screenshot.png",
                "size": 1024,
                "url": "https://cdn.discordapp.com/attachments/1/44/screenshot.png"
            }],
            "poll": {
                "question": {"text": "Deploy to production?"},
                "answers": [
                    {"answer_id": 1, "poll_media": {"text": "Yes", "emoji": {"name": "✅"}}},
                    {"answer_id": 2, "poll_media": {"text": "No"}}
                ],
                "expiry": "2024-01-02T00:00:00+00:00",
                "allow_multiselect": false,
                "layout_type": 1,
                "results": {
                    "is_finalized": true,
                    "answer_counts": [{"id": 1, "count": 2, "me_voted": false}]
                }
            }
        }))
        .expect("documented message with a poll and an attachment");

        let serialised = serde_json::to_value(&message).expect("message serialises");
        let reparsed: Message =
            serde_json::from_value(serialised).expect("the serialised message parses again");

        assert_eq!(reparsed.poll, message.poll);
        assert_eq!(reparsed.attachments, message.attachments);
        assert_eq!(reparsed.content, "Vote!");
    }

    #[test]
    fn message_flags_and_tts_are_typed_and_documented() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "334385199974967042",
            "channel_id": "290926798999357250",
            "author": {"id": "53908099506183680", "username": "Mason"},
            "content": "",
            "tts": true,
            "timestamp": "2017-07-11T17:27:07.299000+00:00",
            "flags": 32832
        }))
        .expect("message with tts and a component-driven V2 flag");

        assert!(message.tts);
        assert_eq!(
            message.flags,
            MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL
        );
        assert!(message.flags.contains(MessageFlags::EPHEMERAL));
        assert!(message.is_ephemeral());
        assert!(message.is_components_v2());

        let serialised = serde_json::to_value(&message).expect("serialisable");
        assert_eq!(serialised["flags"], serde_json::json!(32832));
        assert_eq!(serialised["tts"], serde_json::json!(true));
    }

    #[test]
    fn message_flags_and_tts_default_to_empty_when_absent() {
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "334385199974967042",
            "channel_id": "290926798999357250",
            "author": {"id": "53908099506183680", "username": "Mason"},
            "content": "plain",
            "timestamp": "2017-07-11T17:27:07.299000+00:00"
        }))
        .expect("plain message without flags or tts");

        assert!(!message.tts);
        assert!(message.flags.is_empty());
        assert!(!message.is_ephemeral());
        assert!(!message.is_components_v2());

        let serialised = serde_json::to_value(&message).expect("serialisable");
        // Optional fields default to their documented empty/false values; they
        // are serialised explicitly rather than skipped so serialisation needs
        // no `&bool` predicate (clippy wants tiny `Copy` values passed by value).
        assert_eq!(serialised["flags"], serde_json::json!(0));
        assert_eq!(serialised["tts"], serde_json::json!(false));
    }

    #[test]
    fn message_flags_insert_and_remove() {
        let mut flags = MessageFlags::empty();
        assert!(flags.insert(MessageFlags::EPHEMERAL));
        assert!(!flags.insert(MessageFlags::EPHEMERAL));
        assert_eq!(flags.0, 64);
        assert!(flags.remove(MessageFlags::EPHEMERAL));
        assert!(!flags.remove(MessageFlags::EPHEMERAL));
        assert!(flags.is_empty());
    }
}
