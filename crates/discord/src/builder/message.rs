//! Message builders for `POST /channels/{channel.id}/messages` and for the
//! interaction followup endpoints.
//!
//! The request body is documented in `docs/message.md` (Create Message, JSON
//! Params): `content`, `tts`, `embeds`, `components`, `attachments`,
//! `allowed_mentions`, `flags` and `poll` are the fields the bot needs. The
//! builder never sends `allowed_mentions` unless the caller explicitly adds
//! one, which preserves the bot's current mention behaviour.
//!
//! Attachments are added with [`AttachmentBuilder`]. When at least one file is
//! present the HTTP layer sends the request as `multipart/form-data`: the
//! builder's [`MessageBuilder::to_json`] carries the documented `attachments`
//! metadata array (with each new attachment's `id` equal to its `files[n]`
//! index) and [`MessageBuilder::files`] exposes the files for the encoder
//! (`docs/reference.md`, Uploading Files). A builder without files serialises
//! exactly the JSON body it did before file support existed.
//!
//! Polls are added with [`PollBuilder`], which serialises the documented Poll
//! Create Request Object (`docs/poll.md`, Poll Create Request Object).
//!
//! The documented limits are enforced by clipping: `content` is kept to 2000
//! characters, at most 10 embeds are sent (each inside the Embed Limits of
//! `docs/message.md`), an `allowed_mentions` list holds at most 100 ids, and at
//! most 5 action rows of components are sent.
//!
//! Reference: `docs/message.md` (Create Message), `docs/poll.md` (Poll Create
//! Request Object), `docs/reference.md` (Uploading Files) and
//! `docs/receiving-and-responding.md` (Create Followup Message).

use serde_json::{Map, Value};

use crate::id::{RoleId, UserId};
use crate::model::message::Embed;

use super::attachment::{AttachmentBuilder, attachment_request_metadata};
use super::embed::serialise_embeds;
use super::interaction::EPHEMERAL_FLAG;
use super::limits;
use super::poll::PollBuilder;

/// One documented allowed-mention type: the `parse` array values of the
/// allowed mentions object.
///
/// Reference: `docs/message.md` (Allowed Mention Types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllowedMentionType {
    /// `"roles"`: controls role mentions.
    Roles,
    /// `"users"`: controls user mentions.
    Users,
    /// `"everyone"`: controls `@everyone` and `@here` mentions.
    Everyone,
}

impl AllowedMentionType {
    /// Returns the documented wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Roles => "roles",
            Self::Users => "users",
            Self::Everyone => "everyone",
        }
    }
}

/// Maximum ids in the `roles` and `users` arrays of an allowed mentions
/// object.
///
/// Reference: `docs/message.md` (Allowed Mentions Object).
const ALLOWED_MENTION_IDS: usize = 100;

/// Builds an allowed mentions object, which filters which mentions in a message
/// can ping.
///
/// Reference: `docs/message.md` (Allowed Mentions Object).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllowedMentionsBuilder {
    parse: Option<Vec<AllowedMentionType>>,
    roles: Vec<RoleId>,
    users: Vec<UserId>,
    replied_user: Option<bool>,
}

impl AllowedMentionsBuilder {
    /// Creates an object that lets Discord apply its defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            parse: None,
            roles: Vec::new(),
            users: Vec::new(),
            replied_user: None,
        }
    }

    /// Creates the documented "do not parse any mentions" object
    /// (`{"parse": []}`).
    ///
    /// Reference: `docs/message.md` (Allowed Mentions Examples).
    #[must_use]
    pub fn none() -> Self {
        Self {
            parse: Some(Vec::new()),
            ..Self::new()
        }
    }

    /// Sets the mention types Discord parses from the content.
    ///
    /// `parse` is mutually exclusive with the explicit id lists and with
    /// `replied_user` on the wire (`docs/message.md`, Allowed Mentions
    /// Examples); this builder does not enforce that exclusion.
    #[must_use]
    pub fn parse(mut self, types: impl IntoIterator<Item = AllowedMentionType>) -> Self {
        self.parse.get_or_insert_with(Vec::new).extend(types);
        self
    }

    /// Mentions the given roles explicitly (at most 100 are serialised).
    #[must_use]
    pub fn role_ids(mut self, roles: impl IntoIterator<Item = RoleId>) -> Self {
        self.roles.extend(roles);
        self
    }

    /// Mentions the given users explicitly (at most 100 are serialised).
    #[must_use]
    pub fn user_ids(mut self, users: impl IntoIterator<Item = UserId>) -> Self {
        self.users.extend(users);
        self
    }

    /// Whether to mention the author of the replied-to message.
    #[must_use]
    pub const fn replied_user(mut self, replied_user: bool) -> Self {
        self.replied_user = Some(replied_user);
        self
    }

    /// Serialises the allowed mentions object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();

        if let Some(parse) = &self.parse {
            object.insert(
                "parse".to_owned(),
                Value::Array(
                    parse
                        .iter()
                        .map(|kind| Value::String(kind.as_str().to_owned()))
                        .collect(),
                ),
            );
        }

        if !self.roles.is_empty() {
            object.insert(
                "roles".to_owned(),
                Value::Array(
                    self.roles
                        .iter()
                        .take(ALLOWED_MENTION_IDS)
                        .map(|id| Value::String(id.to_string()))
                        .collect(),
                ),
            );
        }

        if !self.users.is_empty() {
            object.insert(
                "users".to_owned(),
                Value::Array(
                    self.users
                        .iter()
                        .take(ALLOWED_MENTION_IDS)
                        .map(|id| Value::String(id.to_string()))
                        .collect(),
                ),
            );
        }

        if let Some(replied_user) = self.replied_user {
            object.insert("replied_user".to_owned(), Value::Bool(replied_user));
        }

        Value::Object(object)
    }
}

/// Builds the JSON body of a Create Message request.
///
/// ```
/// use discord::builder::message::MessageBuilder;
/// use discord::builder::embed::EmbedBuilder;
///
/// let body = MessageBuilder::new()
///     .content("Hello, world!")
///     .embed(EmbedBuilder::new().title("Hi"))
///     .to_json();
///
/// assert_eq!(body["content"], "Hello, world!");
/// ```
///
/// Reference: `docs/message.md` (Create Message).
#[derive(Clone, Debug, Default)]
pub struct MessageBuilder {
    content: Option<String>,
    embeds: Vec<Embed>,
    components: Vec<super::component::MessageComponentBuilder>,
    tts: Option<bool>,
    flags: Option<u64>,
    allowed_mentions: Option<AllowedMentionsBuilder>,
    poll: Option<PollBuilder>,
    attachments: Vec<AttachmentBuilder>,
}

impl MessageBuilder {
    /// Creates an empty message body.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            content: None,
            embeds: Vec::new(),
            components: Vec::new(),
            tts: None,
            flags: None,
            allowed_mentions: None,
            poll: None,
            attachments: Vec::new(),
        }
    }

    /// Sets the message content (maximum 2000 characters).
    ///
    /// Reference: `docs/message.md` (Create Message, `content`).
    #[must_use]
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Appends one embed (at most 10 are sent).
    ///
    /// Reference: `docs/message.md` (Create Message, `embeds`, Embed Limits).
    #[must_use]
    pub fn embed(mut self, embed: impl Into<Embed>) -> Self {
        self.embeds.push(embed.into());
        self
    }

    /// Appends the given embeds (at most 10 are sent).
    ///
    /// Reference: `docs/message.md` (Create Message, `embeds`, Embed Limits).
    #[must_use]
    pub fn embeds(mut self, embeds: impl IntoIterator<Item = Embed>) -> Self {
        self.embeds.extend(embeds);
        self
    }

    /// Appends one component: an action row for the legacy layout, or any
    /// `IS_COMPONENTS_V2` component such as a container, section or text
    /// display.
    ///
    /// When an `IS_COMPONENTS_V2` component is present, [`Self::to_json`] adds
    /// the documented flag and omits the message content fields the API rejects
    /// alongside it (`docs/message.md`, Create Message footnote).
    ///
    /// Reference: `docs/components-reference.md` (Component Object).
    #[must_use]
    pub fn component(
        mut self,
        component: impl Into<super::component::MessageComponentBuilder>,
    ) -> Self {
        self.components.push(component.into());
        self
    }

    /// Appends components; at most 40 are serialised.
    ///
    /// Reference: `docs/components-reference.md` (Component Object; messages
    /// allow up to 40 total components).
    #[must_use]
    pub fn components<C>(mut self, components: impl IntoIterator<Item = C>) -> Self
    where
        C: Into<super::component::MessageComponentBuilder>,
    {
        self.components
            .extend(components.into_iter().map(Into::into));
        self
    }

    /// Whether the message is text-to-speech (followup messages only).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Followup Message)
    /// and `docs/message.md` (Create Message, `tts`).
    #[must_use]
    pub const fn tts(mut self, tts: bool) -> Self {
        self.tts = Some(tts);
        self
    }

    /// Sets the message flags bitfield.
    ///
    /// Reference: `docs/message.md` (Message Flags).
    #[must_use]
    pub const fn flags(mut self, flags: u64) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Makes the message ephemeral (message flag `EPHEMERAL`, `1 << 6`).
    ///
    /// Reference: `docs/message.md` (Message Flags, `EPHEMERAL`).
    #[must_use]
    pub const fn ephemeral(mut self) -> Self {
        let flags = match self.flags {
            Some(flags) => flags | EPHEMERAL_FLAG,
            None => EPHEMERAL_FLAG,
        };
        self.flags = Some(flags);
        self
    }

    /// Sets the [`AllowedMentionsBuilder`] that filters which mentions ping.
    ///
    /// Reference: `docs/message.md` (Allowed Mentions Object).
    #[must_use]
    pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentionsBuilder) -> Self {
        self.allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Sets the poll attached to the message.
    ///
    /// A poll counts as message content, so a request may contain only a poll
    /// (`docs/message.md`, Create Message footnote: at least one of `content`,
    /// `embeds`, `sticker_ids`, `components`, `files[n]`, `poll` or
    /// `shared_client_theme` is required).
    ///
    /// Reference: `docs/poll.md` (Poll Create Request Object).
    #[must_use]
    pub fn poll(mut self, poll: PollBuilder) -> Self {
        self.poll = Some(poll);
        self
    }

    /// Appends one file to upload.
    ///
    /// When at least one file is present the request is sent as
    /// `multipart/form-data`; the `attachments` metadata array is emitted by
    /// [`Self::to_json`] and the files are exposed by [`Self::files`].
    ///
    /// Reference: `docs/reference.md` (Uploading Files) and `docs/message.md`
    /// (Create Message, `attachments`).
    #[must_use]
    pub fn attachment(mut self, attachment: AttachmentBuilder) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Appends several files to upload.
    ///
    /// Reference: `docs/reference.md` (Uploading Files).
    #[must_use]
    pub fn attachments(mut self, attachments: impl IntoIterator<Item = AttachmentBuilder>) -> Self {
        self.attachments.extend(attachments);
        self
    }

    /// Returns the files to upload, in `files[n]` order.
    ///
    /// The HTTP layer sends the message as `multipart/form-data` when this is
    /// non-empty; the JSON returned by [`Self::to_json`] already contains the
    /// matching `attachments` metadata array.
    #[must_use]
    pub fn files(&self) -> &[AttachmentBuilder] {
        &self.attachments
    }

    /// Returns the JSON payload and the files to upload, as one pair.
    ///
    /// The first element is exactly [`Self::to_json`] and the second exactly
    /// [`Self::files`]; the HTTP layer uses the pair to choose between an
    /// `application/json` request and a `multipart/form-data` request without
    /// changing the JSON-only meaning of `to_json`.
    ///
    /// Reference: `docs/reference.md` (Uploading Files).
    #[must_use]
    pub fn to_payload(&self) -> (Value, &[AttachmentBuilder]) {
        (self.to_json(), self.files())
    }

    /// Serialises the request body.
    ///
    /// Shape: `{"content"?, "tts"?, "embeds"?, "components"?, "attachments"?,
    /// "allowed_mentions"?, "flags"?, "poll"?}`; absent parts are omitted so
    /// the request stays as small as the API allows. The `attachments` array is
    /// only present when files were added: its entries carry the documented
    /// Attachment Request metadata and their `id` is the `files[n]` index the
    /// HTTP layer uses.
    ///
    /// When any component is an `IS_COMPONENTS_V2` component, the documented
    /// `IS_COMPONENTS_V2` flag is added and `content`, `tts`, `embeds` and
    /// `poll` are omitted, because the API rejects them alongside that flag
    /// (`docs/message.md`, Create Message footnote).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();

        let is_v2 = self.requires_v2();

        if !is_v2 {
            if let Some(content) = &self.content {
                object.insert(
                    "content".to_owned(),
                    Value::String(limits::truncate(content, limits::MESSAGE_CONTENT)),
                );
            }

            if let Some(tts) = self.tts {
                object.insert("tts".to_owned(), Value::Bool(tts));
            }

            if !self.embeds.is_empty() {
                object.insert(
                    "embeds".to_owned(),
                    Value::Array(serialise_embeds(&self.embeds)),
                );
            }
        }

        if !self.components.is_empty() {
            let components = if is_v2 {
                super::component::serialise_components(&self.components)
            } else {
                self.components
                    .iter()
                    .take(limits::ACTION_ROWS)
                    .map(super::component::MessageComponentBuilder::to_json)
                    .collect()
            };
            object.insert("components".to_owned(), Value::Array(components));
        }

        if !self.attachments.is_empty() {
            object.insert(
                "attachments".to_owned(),
                attachment_request_metadata(&self.attachments),
            );
        }

        if let Some(allowed_mentions) = &self.allowed_mentions {
            object.insert("allowed_mentions".to_owned(), allowed_mentions.to_json());
        }

        let flags = match (self.flags, is_v2) {
            (Some(flags), true) => Some(flags | super::component::IS_COMPONENTS_V2_FLAG),
            (None, true) => Some(super::component::IS_COMPONENTS_V2_FLAG),
            (flags, false) => flags,
        };
        if let Some(flags) = flags {
            object.insert("flags".to_owned(), Value::from(flags));
        }

        if !is_v2 && let Some(poll) = &self.poll {
            object.insert("poll".to_owned(), poll.to_json());
        }

        Value::Object(object)
    }

    /// Returns true when any component is an `IS_COMPONENTS_V2` component.
    fn requires_v2(&self) -> bool {
        self.components
            .iter()
            .any(super::component::MessageComponentBuilder::requires_v2)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::builder::embed::{EmbedAuthorBuilder, EmbedBuilder};
    use crate::id::{RoleId, UserId};
    use crate::model::message::Embed;
    use crate::timestamp::Timestamp;

    use super::{AllowedMentionType, AllowedMentionsBuilder, MessageBuilder};

    fn embed_with_author_and_timestamp() -> Embed {
        EmbedBuilder::new()
            .author(EmbedAuthorBuilder::new("Example Bot"))
            .color(0x58_65_F2)
            .timestamp(Timestamp::from_unix_millis(1_437_152_400_000))
            .title("Deployed")
            .build()
    }

    #[test]
    fn content_and_one_embed_serialise_to_the_documented_body() {
        let body = MessageBuilder::new()
            .content("Deploy finished.")
            .embed(embed_with_author_and_timestamp())
            .to_json();

        assert_eq!(
            body,
            json!({
                "content": "Deploy finished.",
                "embeds": [{
                    "title": "Deployed",
                    "color": 5_793_266,
                    "timestamp": "2015-07-17T17:00:00.000+00:00",
                    "author": {"name": "Example Bot"}
                }]
            })
        );
    }

    #[test]
    fn absent_parts_are_omitted() {
        assert_eq!(MessageBuilder::new().to_json(), json!({}));
        assert_eq!(
            MessageBuilder::new().content("only content").to_json(),
            json!({"content": "only content"})
        );
        assert_eq!(
            MessageBuilder::new()
                .embed(embed_with_author_and_timestamp())
                .to_json(),
            json!({
                "embeds": [{
                    "title": "Deployed",
                    "color": 5_793_266,
                    "timestamp": "2015-07-17T17:00:00.000+00:00",
                    "author": {"name": "Example Bot"}
                }]
            })
        );
    }

    #[test]
    fn content_is_clipped_to_two_thousand_characters() {
        let body = MessageBuilder::new().content("a".repeat(2_500)).to_json();
        let content = body["content"].as_str().expect("content string");

        assert_eq!(content.chars().count(), 2000);
    }

    #[test]
    fn content_is_clipped_by_characters_not_bytes() {
        let body = MessageBuilder::new().content("🎉".repeat(2_100)).to_json();
        let content = body["content"].as_str().expect("content string");

        assert_eq!(content.chars().count(), 2000);
        assert_eq!(content.len(), 8_000);
    }

    #[test]
    fn at_most_ten_embeds_are_sent() {
        let mut builder = MessageBuilder::new();

        for _ in 0..12 {
            builder = builder.embed(EmbedBuilder::new().title("t"));
        }

        let body = builder.to_json();

        assert_eq!(body["embeds"].as_array().map(Vec::len), Some(10));
    }

    #[test]
    fn the_combined_embed_character_limit_is_enforced() {
        // Three descriptions of 4096 characters each: only 6000 characters
        // may be sent across all embeds (docs/message.md, Embed Limits).
        let mut builder = MessageBuilder::new();

        for _ in 0..3 {
            builder = builder.embed(EmbedBuilder::new().description("d".repeat(4_096)));
        }

        let body = builder.to_json();
        let embeds = body["embeds"].as_array().expect("embeds array");

        let total: usize = embeds
            .iter()
            .filter_map(|embed| embed["description"].as_str())
            .map(|description| description.chars().count())
            .sum();

        assert_eq!(total, 6000);
        assert_eq!(embeds[0]["description"].as_str().map(str::len), Some(4096));
        assert_eq!(embeds[1]["description"].as_str().map(str::len), Some(1904));
        assert_eq!(embeds[2]["description"], json!(""));
    }

    #[test]
    fn embeds_can_be_added_in_bulk() {
        let body = MessageBuilder::new()
            .embeds([
                EmbedBuilder::new().title("one").build(),
                EmbedBuilder::new().title("two").build(),
            ])
            .to_json();

        assert_eq!(body["embeds"][0]["title"], json!("one"));
        assert_eq!(body["embeds"][1]["title"], json!("two"));
    }

    #[test]
    fn default_builder_matches_new() {
        assert_eq!(
            MessageBuilder::default().to_json(),
            MessageBuilder::new().to_json()
        );
    }

    #[test]
    fn allowed_mentions_none_suppresses_every_mention() {
        let body = MessageBuilder::new()
            .content("@everyone hello")
            .allowed_mentions(AllowedMentionsBuilder::none())
            .to_json();

        assert_eq!(body["allowed_mentions"], json!({"parse": []}));
    }

    #[test]
    fn allowed_mentions_parse_and_id_lists_serialise() {
        let body = MessageBuilder::new()
            .allowed_mentions(
                AllowedMentionsBuilder::new()
                    .parse([AllowedMentionType::Users, AllowedMentionType::Roles])
                    .user_ids([UserId::new(1_234)])
                    .role_ids([RoleId::new(5_678)])
                    .replied_user(false),
            )
            .to_json();

        assert_eq!(
            body["allowed_mentions"],
            json!({
                "parse": ["users", "roles"],
                "roles": ["5678"],
                "users": ["1234"],
                "replied_user": false
            })
        );
    }

    #[test]
    fn an_unconfigured_allowed_mentions_object_is_empty() {
        assert_eq!(
            AllowedMentionsBuilder::new().to_json(),
            json!({}),
            "only the fields the caller set are serialised"
        );
    }

    #[test]
    fn allowed_mention_id_lists_are_capped_at_one_hundred() {
        let users = (1..=150).map(UserId::new);
        let body = AllowedMentionsBuilder::new().user_ids(users).to_json();

        assert_eq!(body["users"].as_array().map(Vec::len), Some(100));
    }

    #[test]
    fn message_components_serialise_and_are_capped_at_five_rows() {
        let mut builder = MessageBuilder::new().content("hi");

        for _ in 0..7 {
            builder = builder.component(super::super::interaction::ActionRowBuilder::new().button(
                super::super::interaction::ButtonBuilder::primary("button", "Button"),
            ));
        }

        let body = builder.to_json();
        assert_eq!(body["components"].as_array().map(Vec::len), Some(5));
        assert_eq!(body["components"][0]["type"], json!(1));
    }

    #[test]
    fn tts_flags_and_ephemeral_serialise() {
        assert_eq!(
            MessageBuilder::new().tts(true).to_json(),
            json!({"tts": true})
        );
        assert_eq!(
            MessageBuilder::new().ephemeral().to_json(),
            json!({"flags": 64})
        );
        assert_eq!(
            MessageBuilder::new().flags(4_096).ephemeral().to_json(),
            json!({"flags": 4_160}),
            "the ephemeral helper ORs its bit into an existing flag set"
        );
    }

    #[test]
    fn a_poll_serialises_in_the_create_message_body() {
        use crate::builder::poll::PollBuilder;

        let body = MessageBuilder::new()
            .content("Vote now")
            .poll(
                PollBuilder::new("Deploy to production?")
                    .answer("Yes")
                    .answer("No")
                    .duration_hours(24)
                    .allow_multiselect(false),
            )
            .to_json();

        assert_eq!(body["content"], json!("Vote now"));
        assert_eq!(
            body["poll"],
            json!({
                "question": {"text": "Deploy to production?"},
                "answers": [
                    {"poll_media": {"text": "Yes"}},
                    {"poll_media": {"text": "No"}}
                ],
                "duration": 24,
                "allow_multiselect": false
            })
        );
    }

    #[test]
    fn a_poll_can_be_the_only_message_content() {
        use crate::builder::poll::PollBuilder;

        let body = MessageBuilder::new()
            .poll(PollBuilder::new("Pick one").answer("A").answer("B"))
            .to_json();

        assert_eq!(body.as_object().map(serde_json::Map::len), Some(1));
        assert!(body.get("poll").is_some());
    }

    #[test]
    fn json_only_bodies_still_have_no_attachments_key() {
        let body = MessageBuilder::new().content("no files").to_json();

        assert_eq!(body, json!({"content": "no files"}));
        assert!(
            MessageBuilder::new().files().is_empty(),
            "a builder without files has no files to upload"
        );
    }

    #[test]
    fn attachments_metadata_matches_the_uploaded_files() {
        use crate::builder::attachment::AttachmentBuilder;

        let builder = MessageBuilder::new()
            .content("Here are the files")
            .attachment(AttachmentBuilder::new("one.png", [1_u8, 2, 3]))
            .attachment(AttachmentBuilder::new("two.txt", [4_u8]).with_description("A text file"))
            .attachments([AttachmentBuilder::new("three.gif", [5_u8])]);

        assert_eq!(builder.files().len(), 3);
        assert_eq!(builder.files()[0].filename(), "one.png");
        assert_eq!(builder.files()[1].description(), Some("A text file"));

        let body = builder.to_json();

        assert_eq!(
            body["attachments"],
            json!([
                {"id": 0, "filename": "one.png"},
                {"id": 1, "filename": "two.txt", "description": "A text file"},
                {"id": 2, "filename": "three.gif"}
            ]),
            "each new attachment's id is its files[n] index"
        );
        assert_eq!(body["content"], json!("Here are the files"));
    }

    #[test]
    fn a_file_can_be_sent_without_any_other_content() {
        use crate::builder::attachment::AttachmentBuilder;

        let builder = MessageBuilder::new().attachment(AttachmentBuilder::new("only.txt", b"x"));
        let body = builder.to_json();

        assert_eq!(
            body,
            json!({"attachments": [{"id": 0, "filename": "only.txt"}]})
        );
        assert!(
            !builder.files().is_empty(),
            "the HTTP layer must switch to multipart when files are present"
        );

        let (payload, files) = builder.to_payload();
        assert_eq!(payload, body, "to_payload JSON is exactly to_json");
        assert_eq!(
            files,
            builder.files(),
            "to_payload files are exactly files()"
        );
    }
}
