//! Interaction response and component builders.
//!
//! An interaction is acknowledged by sending a callback body to
//! `POST /interactions/{interaction_id}/{interaction_token}/callback`; the body
//! is an interaction response object `{"type": ..., "data": {...}}`. The
//! documented callback types are in `docs/receiving-and-responding.md`
//! (Interaction Callback Type):
//!
//! | Type | Builder |
//! | - | - |
//! | `PONG` (`1`) | [`InteractionResponse::Pong`] |
//! | `CHANNEL_MESSAGE_WITH_SOURCE` (`4`) | [`ReplyBuilder`] |
//! | `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE` (`5`) | [`DeferBuilder`] |
//! | `DEFERRED_UPDATE_MESSAGE` (`6`) | [`DeferUpdateBuilder`] |
//! | `UPDATE_MESSAGE` (`7`) | [`UpdateMessageBuilder`] |
//! | `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT` (`8`) | [`AutocompleteBuilder`] |
//! | `MODAL` (`9`) | [`ModalBuilder`] |
//! | `PREMIUM_REQUIRED` (`10`, deprecated) | [`InteractionResponse::PremiumRequired`] |
//! | `LAUNCH_ACTIVITY` (`12`) | [`InteractionResponse::LaunchActivity`] |
//!
//! Message components are documented in `docs/components-reference.md`
//! (Action Row, Button, String Select, Text Input, User Select, Role Select,
//! Mentionable Select, Channel Select, Label) and are composed with
//! [`ActionRowBuilder`], [`LabelBuilder`] and the per-component builders. The
//! documented limits are enforced by clipping: at most 5 buttons or exactly one
//! select per action row, at most 5 action rows per message, 5 text inputs per
//! legacy modal action row, 1-5 modal components, at most 25 autocomplete
//! choices, `custom_id` 1-100 characters and a modal title of at most 45
//! characters.
//!
//! Ephemeral messages use message flag `EPHEMERAL` (`1 << 6`), documented in
//! `docs/message.md` (Message Flags). Buttons are message components: an action
//! row (component type 1) holds up to 5 buttons (component type 2); link
//! buttons use style 5 and a `url` instead of a `custom_id`, premium buttons
//! use style 6 and a `sku_id`, and their emoji is the partial emoji object
//! (`name`, `id`, `animated`) documented for the button `emoji` field in
//! `docs/components-reference.md` (Button Structure), with the custom emoji
//! example in `docs/emoji.md` (Custom Emoji Examples).
//!
//! The edit builder produces the body of
//! `PATCH /webhooks/{application_id}/{interaction_token}/messages/@original`
//! (`docs/receiving-and-responding.md`, Edit Original Interaction Response):
//! `{"content"?, "embeds"?, "components"?, "allowed_mentions"?,
//! "attachments"?, "poll"?}`.
//!
//! Message callbacks support file uploads and polls like the create-message
//! endpoint: [`ReplyBuilder`] and [`UpdateMessageBuilder`] carry
//! [`AttachmentBuilder`] files (the metadata array goes inside `data`) and a
//! [`PollBuilder`] (`docs/receiving-and-responding.md`, Interaction Callback
//! Data Structure, Messages). The edit endpoints support files too, because
//! they are documented to function like Edit Webhook Message
//! (`docs/webhook.md`, Edit Webhook Message: `files[n]`, `payload_json` and
//! `attachments`); `poll` is accepted there only when editing a deferred
//! interaction response (`docs/webhook.md`, Edit Webhook Message footnote 4).

use serde_json::{Map, Value};

use crate::id::AttachmentId;
use crate::model::application::InteractionCallbackType;
use crate::model::component::{ComponentKind, TextInputStyle};
use crate::model::emoji::ReactionType;
use crate::model::message::Embed;

use super::attachment::{AttachmentBuilder, attachment_request_metadata};
use super::component::{IS_COMPONENTS_V2_FLAG, MAX_MESSAGE_COMPONENTS, MessageComponentBuilder};
use super::embed::serialise_embeds;
use super::limits;
use super::message::AllowedMentionsBuilder;
use super::poll::PollBuilder;

/// The ephemeral message flag: `1 << 6`.
///
/// Reference: `docs/message.md` (Message Flags, `EPHEMERAL`).
pub(crate) const EPHEMERAL_FLAG: u64 = 1 << 6;

/// Serialises a callback body from its type and data.
fn callback(kind: InteractionCallbackType, data: Option<Map<String, Value>>) -> Value {
    let mut body = Map::new();
    body.insert("type".to_owned(), Value::from(kind.as_i64()));

    if let Some(data) = data {
        body.insert("data".to_owned(), Value::Object(data));
    }

    Value::Object(body)
}

/// Adds the message flags field when an ephemeral flag is set.
fn insert_ephemeral(data: &mut Map<String, Value>, ephemeral: bool) {
    if ephemeral {
        data.insert("flags".to_owned(), Value::from(EPHEMERAL_FLAG));
    }
}

/// Builds an interaction message reply (callback type 4).
///
/// ```
/// use discord::builder::interaction::{ButtonBuilder, ReplyBuilder};
///
/// let body = ReplyBuilder::new()
///     .content("Pong!")
///     .ephemeral()
///     .button(ButtonBuilder::link("https://example.com", "Website"))
///     .to_json();
///
/// assert_eq!(body["type"], 4);
/// assert_eq!(body["data"]["flags"], 64);
/// ```
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
/// Structure, Messages).
#[derive(Clone, Debug, Default)]
pub struct ReplyBuilder {
    content: Option<String>,
    embeds: Vec<Embed>,
    buttons: Vec<ButtonBuilder>,
    components: Vec<MessageComponentBuilder>,
    ephemeral: bool,
    tts: Option<bool>,
    allowed_mentions: Option<AllowedMentionsBuilder>,
    poll: Option<PollBuilder>,
    attachments: Vec<AttachmentBuilder>,
}

impl ReplyBuilder {
    /// Creates an empty reply.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            content: None,
            embeds: Vec::new(),
            buttons: Vec::new(),
            components: Vec::new(),
            ephemeral: false,
            tts: None,
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

    /// Appends one embed.
    ///
    /// Reference: `docs/message.md` (Embed Object, Embed Limits).
    #[must_use]
    pub fn embed(mut self, embed: impl Into<Embed>) -> Self {
        self.embeds.push(embed.into());
        self
    }

    /// Appends one button; buttons are grouped into action rows of at most
    /// five, and at most 5 rows (25 buttons) are serialised.
    ///
    /// Prefer [`Self::components`] for selects and mixed component layouts.
    #[must_use]
    pub fn button(mut self, button: ButtonBuilder) -> Self {
        self.buttons.push(button);
        self
    }

    /// Appends one component (an action row for the legacy layout, or any
    /// `IS_COMPONENTS_V2` component).
    ///
    /// When an `IS_COMPONENTS_V2` component is present, the callback carries
    /// only components plus the documented flag; the API rejects the message
    /// content fields alongside it (`docs/message.md`, Create Message footnote).
    ///
    /// Reference: `docs/components-reference.md` (Component Object).
    #[must_use]
    pub fn component(mut self, component: impl Into<MessageComponentBuilder>) -> Self {
        self.components.push(component.into());
        self
    }

    /// Appends components.
    ///
    /// At most 5 action rows are serialised for legacy replies, counting the
    /// rows the [`Self::button`] helper groups buttons into; `IS_COMPONENTS_V2`
    /// replies allow up to 40 components.
    #[must_use]
    pub fn components<C>(mut self, components: impl IntoIterator<Item = C>) -> Self
    where
        C: Into<MessageComponentBuilder>,
    {
        self.components
            .extend(components.into_iter().map(Into::into));
        self
    }

    /// Whether the reply is text-to-speech.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
    /// Structure, Messages, `tts`).
    #[must_use]
    pub const fn tts(mut self, tts: bool) -> Self {
        self.tts = Some(tts);
        self
    }

    /// Sets the [`AllowedMentionsBuilder`] that filters which mentions ping.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
    /// Structure, Messages, `allowed_mentions`) and `docs/message.md` (Allowed
    /// Mentions Object).
    #[must_use]
    pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentionsBuilder) -> Self {
        self.allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Sets the poll attached to the reply.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
    /// Structure, Messages, `poll`) and `docs/poll.md` (Poll Create Request
    /// Object).
    #[must_use]
    pub fn poll(mut self, poll: PollBuilder) -> Self {
        self.poll = Some(poll);
        self
    }

    /// Appends one file to upload with the reply.
    ///
    /// When at least one file is present the HTTP layer sends the callback as
    /// `multipart/form-data`; [`Self::to_json`] then contains the documented
    /// `data.attachments` metadata array and [`Self::files`] exposes the files.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Interaction
    /// Response: "This endpoint also supports file attachments") and
    /// `docs/reference.md` (Uploading Files).
    #[must_use]
    pub fn attachment(mut self, attachment: AttachmentBuilder) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Appends several files to upload with the reply.
    ///
    /// Reference: `docs/reference.md` (Uploading Files).
    #[must_use]
    pub fn attachments(mut self, attachments: impl IntoIterator<Item = AttachmentBuilder>) -> Self {
        self.attachments.extend(attachments);
        self
    }

    /// Returns the files to upload, in `files[n]` order.
    #[must_use]
    pub fn files(&self) -> &[AttachmentBuilder] {
        &self.attachments
    }

    /// Makes the reply ephemeral (only the invoking user sees it).
    ///
    /// Reference: `docs/message.md` (Message Flags, `EPHEMERAL`).
    #[must_use]
    pub const fn ephemeral(mut self) -> Self {
        self.ephemeral = true;
        self
    }

    /// Sets whether the reply is ephemeral.
    #[must_use]
    pub const fn ephemeral_flag(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }

    /// Serialises the interaction callback body `{"type": 4, "data": {...}}`.
    ///
    /// When an `IS_COMPONENTS_V2` component is attached, the callback carries
    /// only components plus the documented flags: the API rejects the message
    /// content fields alongside `IS_COMPONENTS_V2` (`docs/message.md`, Create
    /// Message footnote).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut data = Map::new();

        if self.requires_v2() {
            data.insert("components".to_owned(), Value::Array(self.v2_components()));

            if !self.attachments.is_empty() {
                data.insert(
                    "attachments".to_owned(),
                    attachment_request_metadata(&self.attachments),
                );
            }

            let mut flags = IS_COMPONENTS_V2_FLAG;
            if self.ephemeral {
                flags |= EPHEMERAL_FLAG;
            }
            data.insert("flags".to_owned(), Value::from(flags));

            // `allowed_mentions` is not one of the fields `IS_COMPONENTS_V2`
            // rejects (`docs/message.md`, Create Message footnote), and it is
            // what filters the mentions inside text display components
            // (`docs/components-reference.md`, Text Display), so it is kept.
            if let Some(allowed_mentions) = &self.allowed_mentions {
                data.insert("allowed_mentions".to_owned(), allowed_mentions.to_json());
            }

            return callback(
                InteractionCallbackType::ChannelMessageWithSource,
                Some(data),
            );
        }

        if let Some(content) = &self.content {
            data.insert(
                "content".to_owned(),
                Value::String(limits::truncate(content, limits::MESSAGE_CONTENT)),
            );
        }

        if let Some(tts) = self.tts {
            data.insert("tts".to_owned(), Value::Bool(tts));
        }

        if !self.embeds.is_empty() {
            data.insert(
                "embeds".to_owned(),
                Value::Array(serialise_embeds(&self.embeds)),
            );
        }

        if !self.buttons.is_empty() || !self.components.is_empty() {
            data.insert("components".to_owned(), Value::Array(self.component_rows()));
        }

        if let Some(allowed_mentions) = &self.allowed_mentions {
            data.insert("allowed_mentions".to_owned(), allowed_mentions.to_json());
        }

        if !self.attachments.is_empty() {
            data.insert(
                "attachments".to_owned(),
                attachment_request_metadata(&self.attachments),
            );
        }

        if let Some(poll) = &self.poll {
            data.insert("poll".to_owned(), poll.to_json());
        }

        insert_ephemeral(&mut data, self.ephemeral);

        callback(
            InteractionCallbackType::ChannelMessageWithSource,
            Some(data),
        )
    }

    /// Returns true when any attached component is an `IS_COMPONENTS_V2`
    /// component.
    fn requires_v2(&self) -> bool {
        self.components
            .iter()
            .any(MessageComponentBuilder::requires_v2)
    }

    /// Serialises all attached components for an `IS_COMPONENTS_V2` reply.
    fn v2_components(&self) -> Vec<Value> {
        let mut components = button_action_rows(&self.buttons);
        components.extend(self.components.iter().map(MessageComponentBuilder::to_json));
        components.truncate(MAX_MESSAGE_COMPONENTS);
        components
    }

    /// Returns the serialised action rows: the button helper first, the
    /// explicitly added rows after it, both capped at the documented maximum.
    fn component_rows(&self) -> Vec<Value> {
        let mut rows = button_action_rows(&self.buttons);

        for row in self
            .components
            .iter()
            .take(limits::ACTION_ROWS.saturating_sub(rows.len()))
        {
            rows.push(row.to_json());
        }

        rows
    }
}

/// Builds a deferred message reply (callback type 5).
///
/// A defer without the ephemeral flag is a public "thinking" reply; with the
/// flag only the invoking user sees the thinking state.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
/// `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE`).
#[derive(Clone, Debug, Default)]
pub struct DeferBuilder {
    ephemeral: bool,
}

impl DeferBuilder {
    /// Creates a public deferred reply.
    #[must_use]
    pub const fn new() -> Self {
        Self { ephemeral: false }
    }

    /// Makes the deferred reply ephemeral.
    #[must_use]
    pub const fn ephemeral(mut self) -> Self {
        self.ephemeral = true;
        self
    }

    /// Sets whether the deferred reply is ephemeral.
    #[must_use]
    pub const fn ephemeral_flag(mut self, ephemeral: bool) -> Self {
        self.ephemeral = ephemeral;
        self
    }

    /// Serialises the interaction callback body `{"type": 5, "data": {...}}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut data = Map::new();
        insert_ephemeral(&mut data, self.ephemeral);

        callback(
            InteractionCallbackType::DeferredChannelMessageWithSource,
            Some(data),
        )
    }
}

/// Builds a deferred message edit (callback type 6).
///
/// The interaction is acknowledged without a loading state; the original
/// message is edited later with
/// `PATCH /webhooks/{application_id}/{interaction_token}/messages/@original`.
/// Only valid for message component and modal submit interactions.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
/// `DEFERRED_UPDATE_MESSAGE`).
#[derive(Clone, Debug, Default)]
pub struct DeferUpdateBuilder;

impl DeferUpdateBuilder {
    /// Creates a deferred update.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Serialises the interaction callback body `{"type": 6}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        callback(InteractionCallbackType::DeferredUpdateMessage, None)
    }
}

/// Builds an edit of the message the component was attached to (callback type
/// 7).
///
/// Only valid for message component and modal submit interactions.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
/// `UPDATE_MESSAGE`, and Interaction Callback Data Structure, Messages).
#[derive(Clone, Debug, Default)]
pub struct UpdateMessageBuilder {
    content: Option<String>,
    embeds: Vec<Embed>,
    components: Vec<MessageComponentBuilder>,
    tts: Option<bool>,
    allowed_mentions: Option<AllowedMentionsBuilder>,
    poll: Option<PollBuilder>,
    attachments: Vec<AttachmentBuilder>,
}

impl UpdateMessageBuilder {
    /// Creates an empty message edit.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            content: None,
            embeds: Vec::new(),
            components: Vec::new(),
            tts: None,
            allowed_mentions: None,
            poll: None,
            attachments: Vec::new(),
        }
    }

    /// Sets the new message content (maximum 2000 characters).
    #[must_use]
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Appends one embed.
    #[must_use]
    pub fn embed(mut self, embed: impl Into<Embed>) -> Self {
        self.embeds.push(embed.into());
        self
    }

    /// Appends one component (an action row for the legacy layout, or any
    /// `IS_COMPONENTS_V2` component).
    #[must_use]
    pub fn component(mut self, component: impl Into<MessageComponentBuilder>) -> Self {
        self.components.push(component.into());
        self
    }

    /// Appends components: at most 5 action rows for legacy layouts, or up to
    /// 40 components for `IS_COMPONENTS_V2`.
    #[must_use]
    pub fn components<C>(mut self, components: impl IntoIterator<Item = C>) -> Self
    where
        C: Into<MessageComponentBuilder>,
    {
        self.components
            .extend(components.into_iter().map(Into::into));
        self
    }

    /// Whether the message is text-to-speech.
    #[must_use]
    pub const fn tts(mut self, tts: bool) -> Self {
        self.tts = Some(tts);
        self
    }

    /// Sets the [`AllowedMentionsBuilder`] that filters which mentions ping.
    #[must_use]
    pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentionsBuilder) -> Self {
        self.allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Sets the poll attached to the edited message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
    /// Structure, Messages, `poll`).
    #[must_use]
    pub fn poll(mut self, poll: PollBuilder) -> Self {
        self.poll = Some(poll);
        self
    }

    /// Appends one file to upload with the edited message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Interaction
    /// Response: file attachments are supported) and `docs/reference.md`
    /// (Uploading Files).
    #[must_use]
    pub fn attachment(mut self, attachment: AttachmentBuilder) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Appends several files to upload with the edited message.
    ///
    /// Reference: `docs/reference.md` (Uploading Files).
    #[must_use]
    pub fn attachments(mut self, attachments: impl IntoIterator<Item = AttachmentBuilder>) -> Self {
        self.attachments.extend(attachments);
        self
    }

    /// Returns the files to upload, in `files[n]` order.
    #[must_use]
    pub fn files(&self) -> &[AttachmentBuilder] {
        &self.attachments
    }

    /// Serialises the interaction callback body `{"type": 7, "data": {...}}`.
    ///
    /// When an `IS_COMPONENTS_V2` component is attached, the callback carries
    /// only components plus the documented flag (`docs/message.md`, Create
    /// Message footnote).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut data = Map::new();

        if self.requires_v2() {
            data.insert("components".to_owned(), Value::Array(self.v2_components()));
            data.insert("flags".to_owned(), Value::from(IS_COMPONENTS_V2_FLAG));

            if !self.attachments.is_empty() {
                data.insert(
                    "attachments".to_owned(),
                    attachment_request_metadata(&self.attachments),
                );
            }

            // `allowed_mentions` is not one of the fields `IS_COMPONENTS_V2`
            // rejects (`docs/message.md`, Create Message footnote) and the edit
            // rules say it still controls mention parsing
            // (`docs/message.md`, Edit Message), so it is kept.
            if let Some(allowed_mentions) = &self.allowed_mentions {
                data.insert("allowed_mentions".to_owned(), allowed_mentions.to_json());
            }

            return callback(InteractionCallbackType::UpdateMessage, Some(data));
        }

        if let Some(content) = &self.content {
            data.insert(
                "content".to_owned(),
                Value::String(limits::truncate(content, limits::MESSAGE_CONTENT)),
            );
        }

        if let Some(tts) = self.tts {
            data.insert("tts".to_owned(), Value::Bool(tts));
        }

        if !self.embeds.is_empty() {
            data.insert(
                "embeds".to_owned(),
                Value::Array(serialise_embeds(&self.embeds)),
            );
        }

        if !self.components.is_empty() {
            data.insert(
                "components".to_owned(),
                Value::Array(
                    self.components
                        .iter()
                        .take(limits::ACTION_ROWS)
                        .map(MessageComponentBuilder::to_json)
                        .collect(),
                ),
            );
        }

        if let Some(allowed_mentions) = &self.allowed_mentions {
            data.insert("allowed_mentions".to_owned(), allowed_mentions.to_json());
        }

        if !self.attachments.is_empty() {
            data.insert(
                "attachments".to_owned(),
                attachment_request_metadata(&self.attachments),
            );
        }

        if let Some(poll) = &self.poll {
            data.insert("poll".to_owned(), poll.to_json());
        }

        callback(InteractionCallbackType::UpdateMessage, Some(data))
    }
}

impl UpdateMessageBuilder {
    /// Returns true when any attached component is an `IS_COMPONENTS_V2`
    /// component.
    fn requires_v2(&self) -> bool {
        self.components
            .iter()
            .any(MessageComponentBuilder::requires_v2)
    }

    /// Serialises all attached components for an `IS_COMPONENTS_V2` edit.
    fn v2_components(&self) -> Vec<Value> {
        self.components
            .iter()
            .take(MAX_MESSAGE_COMPONENTS)
            .map(MessageComponentBuilder::to_json)
            .collect()
    }
}

/// One autocomplete choice value: a string or an integer.
///
/// Reference: `docs/application-commands.md` (Application Command Option Choice
/// Structure) — the value type follows the focused option's type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutocompleteChoiceValue {
    /// A string choice value (for string options).
    String(String),
    /// An integer choice value (for integer options).
    Integer(i64),
}

impl From<String> for AutocompleteChoiceValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for AutocompleteChoiceValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<i64> for AutocompleteChoiceValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl AutocompleteChoiceValue {
    /// Serialises the value as the documented JSON scalar.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::String(value) => Value::String(limits::truncate(value, limits::CHOICE_NAME)),
            Self::Integer(value) => Value::from(*value),
        }
    }
}

/// One autocomplete choice: a `name` and a string or integer `value`.
///
/// Reference: `docs/application-commands.md` (Application Command Option Choice
/// Structure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutocompleteChoice {
    name: String,
    value: AutocompleteChoiceValue,
}

impl AutocompleteChoice {
    /// Creates a choice with the given name and value.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<AutocompleteChoiceValue>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    /// Serialises the choice object `{"name": ..., "value": ...}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "name".to_owned(),
            Value::String(limits::truncate(&self.name, limits::CHOICE_NAME)),
        );
        object.insert("value".to_owned(), self.value.to_json());

        Value::Object(object)
    }
}

/// Builds an autocomplete result (callback type 8).
///
/// At most 25 choices are serialised.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
/// Structure, Autocomplete) and `docs/application-commands.md` (Autocomplete).
#[derive(Clone, Debug, Default)]
pub struct AutocompleteBuilder {
    choices: Vec<AutocompleteChoice>,
}

impl AutocompleteBuilder {
    /// Creates an empty autocomplete result.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            choices: Vec::new(),
        }
    }

    /// Appends one choice.
    #[must_use]
    pub fn choice(
        mut self,
        name: impl Into<String>,
        value: impl Into<AutocompleteChoiceValue>,
    ) -> Self {
        self.choices.push(AutocompleteChoice::new(name, value));
        self
    }

    /// Appends choices.
    #[must_use]
    pub fn choices(mut self, choices: impl IntoIterator<Item = AutocompleteChoice>) -> Self {
        self.choices.extend(choices);
        self
    }

    /// Serialises the interaction callback body
    /// `{"type": 8, "data": {"choices": [...]}}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let choices = self
            .choices
            .iter()
            .take(limits::AUTOCOMPLETE_CHOICES)
            .map(AutocompleteChoice::to_json)
            .collect();

        let mut data = Map::new();
        data.insert("choices".to_owned(), Value::Array(choices));

        callback(
            InteractionCallbackType::ApplicationCommandAutocompleteResult,
            Some(data),
        )
    }
}

/// A component that a modal can hold: a legacy action row or a label.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Response Object,
/// Modal) and `docs/components-reference.md` (Label).
#[derive(Clone, Debug)]
pub enum ModalComponent {
    /// A legacy action row of modal components.
    ActionRow(ActionRowBuilder),
    /// A label wrapping one interactive component.
    Label(LabelBuilder),
}

impl ModalComponent {
    /// Serialises the component.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::ActionRow(row) => row.to_json(),
            Self::Label(label) => label.to_json(),
        }
    }
}

impl From<ActionRowBuilder> for ModalComponent {
    fn from(value: ActionRowBuilder) -> Self {
        Self::ActionRow(value)
    }
}

impl From<LabelBuilder> for ModalComponent {
    fn from(value: LabelBuilder) -> Self {
        Self::Label(value)
    }
}

/// Builds a modal response (callback type 9).
///
/// The `custom_id` is 1-100 characters and the title at most 45; between 1 and
/// 5 components are serialised. Only valid for application command and message
/// component interactions (`docs/receiving-and-responding.md`, Interaction
/// Callback Type footnote: not available for `MODAL_SUBMIT` and `PING`).
///
/// ```
/// use discord::builder::interaction::{LabelBuilder, ModalBuilder, TextInputBuilder};
///
/// let body = ModalBuilder::new("feedback", "Game Feedback")
///     .label(
///         LabelBuilder::new("What did you think?")
///             .text_input(TextInputBuilder::paragraph("feedback").required(true)),
///     )
///     .to_json();
///
/// assert_eq!(body["type"], 9);
/// assert_eq!(body["data"]["custom_id"], "feedback");
/// ```
#[derive(Clone, Debug, Default)]
pub struct ModalBuilder {
    custom_id: String,
    title: String,
    components: Vec<ModalComponent>,
}

impl ModalBuilder {
    /// Creates a modal with the given `custom_id` and title.
    #[must_use]
    pub fn new(custom_id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            title: title.into(),
            components: Vec::new(),
        }
    }

    /// Appends one modal component (an action row or a label).
    #[must_use]
    pub fn component(mut self, component: impl Into<ModalComponent>) -> Self {
        self.components.push(component.into());
        self
    }

    /// Appends modal components (at most 5 are serialised).
    #[must_use]
    pub fn components<I, C>(mut self, components: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<ModalComponent>,
    {
        self.components
            .extend(components.into_iter().map(Into::into));
        self
    }

    /// Appends a label, the current way to wrap a modal input.
    #[must_use]
    pub fn label(self, label: LabelBuilder) -> Self {
        self.component(ModalComponent::Label(label))
    }

    /// Appends a legacy action row of modal components.
    #[must_use]
    pub fn action_row(self, row: ActionRowBuilder) -> Self {
        self.component(ModalComponent::ActionRow(row))
    }

    /// Serialises the interaction callback body
    /// `{"type": 9, "data": {"custom_id", "title", "components"}}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let components = self
            .components
            .iter()
            .take(limits::MODAL_COMPONENTS)
            .map(ModalComponent::to_json)
            .collect();

        let mut data = Map::new();
        data.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );
        data.insert(
            "title".to_owned(),
            Value::String(limits::truncate(&self.title, limits::MODAL_TITLE)),
        );
        data.insert("components".to_owned(), Value::Array(components));

        callback(InteractionCallbackType::Modal, Some(data))
    }
}

/// An interaction response: the callback type plus its optional data.
#[derive(Clone, Debug)]
pub enum InteractionResponse {
    /// A message reply (callback type 4).
    Reply(ReplyBuilder),
    /// A deferred message reply (callback type 5).
    Defer(DeferBuilder),
    /// An acknowledgement of a ping interaction (callback type 1).
    Pong,
    /// A deferred edit of the original message (callback type 6).
    DeferUpdate(DeferUpdateBuilder),
    /// An edit of the message the component was attached to (callback type 7).
    UpdateMessage(UpdateMessageBuilder),
    /// An autocomplete result (callback type 8).
    Autocomplete(AutocompleteBuilder),
    /// A modal (callback type 9).
    Modal(ModalBuilder),
    /// The deprecated premium upgrade response (callback type 10).
    PremiumRequired,
    /// A request to launch the app's Activity (callback type 12).
    LaunchActivity,
}

impl InteractionResponse {
    /// Serialises the interaction callback body.
    ///
    /// Shape: `{"type": ..., "data": {...}}` for the data-carrying callbacks,
    /// and `{"type": 1}` / `{"type": 10}` / `{"type": 12}` for the callbacks
    /// without data (`docs/receiving-and-responding.md`, Interaction Response
    /// Object).
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Reply(reply) => reply.to_json(),
            Self::Defer(defer) => defer.to_json(),
            Self::Pong => callback(InteractionCallbackType::Pong, None),
            Self::DeferUpdate(defer_update) => defer_update.to_json(),
            Self::UpdateMessage(update) => update.to_json(),
            Self::Autocomplete(autocomplete) => autocomplete.to_json(),
            Self::Modal(modal) => modal.to_json(),
            Self::PremiumRequired => callback(InteractionCallbackType::PremiumRequired, None),
            Self::LaunchActivity => callback(InteractionCallbackType::LaunchActivity, None),
        }
    }
    /// Returns the files the response uploads, in `files[n]` order.
    ///
    /// Only the message callbacks ([`Self::Reply`] and
    /// [`Self::UpdateMessage`]) can carry files; every other callback returns an
    /// empty slice. The HTTP layer sends the callback as
    /// `multipart/form-data` when this is non-empty, with the JSON from
    /// [`Self::to_json`] as the `payload_json` part.
    #[must_use]
    pub fn files(&self) -> &[AttachmentBuilder] {
        match self {
            Self::Reply(reply) => reply.files(),
            Self::UpdateMessage(update) => update.files(),
            _ => &[],
        }
    }

    /// Returns the callback JSON and the files to upload, as one pair.
    ///
    /// The first element is exactly [`Self::to_json`] and the second exactly
    /// [`Self::files`]; the HTTP layer uses the pair to choose between a JSON
    /// request and a `multipart/form-data` request.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Interaction
    /// Response) and `docs/reference.md` (Uploading Files).
    #[must_use]
    pub fn to_payload(&self) -> (Value, &[AttachmentBuilder]) {
        (self.to_json(), self.files())
    }
}

impl From<ReplyBuilder> for InteractionResponse {
    fn from(value: ReplyBuilder) -> Self {
        Self::Reply(value)
    }
}

impl From<DeferBuilder> for InteractionResponse {
    fn from(value: DeferBuilder) -> Self {
        Self::Defer(value)
    }
}

impl From<DeferUpdateBuilder> for InteractionResponse {
    fn from(value: DeferUpdateBuilder) -> Self {
        Self::DeferUpdate(value)
    }
}

impl From<UpdateMessageBuilder> for InteractionResponse {
    fn from(value: UpdateMessageBuilder) -> Self {
        Self::UpdateMessage(value)
    }
}

impl From<AutocompleteBuilder> for InteractionResponse {
    fn from(value: AutocompleteBuilder) -> Self {
        Self::Autocomplete(value)
    }
}

impl From<ModalBuilder> for InteractionResponse {
    fn from(value: ModalBuilder) -> Self {
        Self::Modal(value)
    }
}

/// Builds the body of an interaction response edit request
/// (`PATCH /webhooks/{application_id}/{interaction_token}/messages/@original`).
///
/// The endpoints this builder feeds are documented to function like Edit
/// Webhook Message, which accepts file uploads (`files[n]`, `payload_json`,
/// `attachments`) and, only when editing a deferred interaction response, a
/// `poll` (`docs/webhook.md`, Edit Webhook Message). A builder without files or
/// a poll serialises exactly the edit body it did before those capabilities
/// existed.
///
/// Reference: `docs/receiving-and-responding.md` (Edit Original Interaction
/// Response) and `docs/webhook.md` (Edit Webhook Message).
#[derive(Clone, Debug, Default)]
pub struct InteractionEditBuilder {
    content: Option<String>,
    embeds: Vec<Embed>,
    components: Vec<MessageComponentBuilder>,
    allowed_mentions: Option<AllowedMentionsBuilder>,
    poll: Option<PollBuilder>,
    attachments: Vec<AttachmentBuilder>,
    kept_attachments: Vec<AttachmentId>,
}

impl InteractionEditBuilder {
    /// Creates an empty edit body.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            content: None,
            embeds: Vec::new(),
            components: Vec::new(),
            allowed_mentions: None,
            poll: None,
            attachments: Vec::new(),
            kept_attachments: Vec::new(),
        }
    }

    /// Sets the new message content (maximum 2000 characters).
    #[must_use]
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = Some(content.into());
        self
    }

    /// Appends one embed.
    #[must_use]
    pub fn embed(mut self, embed: impl Into<Embed>) -> Self {
        self.embeds.push(embed.into());
        self
    }

    /// Appends one component (an action row for the legacy layout, or any
    /// `IS_COMPONENTS_V2` component).
    #[must_use]
    pub fn component(mut self, component: impl Into<MessageComponentBuilder>) -> Self {
        self.components.push(component.into());
        self
    }

    /// Appends components: at most 5 action rows for legacy layouts, or up to
    /// 40 components for `IS_COMPONENTS_V2`.
    #[must_use]
    pub fn components<C>(mut self, components: impl IntoIterator<Item = C>) -> Self
    where
        C: Into<MessageComponentBuilder>,
    {
        self.components
            .extend(components.into_iter().map(Into::into));
        self
    }

    /// Sets the [`AllowedMentionsBuilder`] that filters which mentions ping.
    #[must_use]
    pub fn allowed_mentions(mut self, allowed_mentions: AllowedMentionsBuilder) -> Self {
        self.allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Sets the poll attached to the edited response.
    ///
    /// `docs/webhook.md` (Edit Webhook Message, footnote 4) says polls can only
    /// be added when editing a deferred interaction response; the endpoint
    /// rejects a poll added to any other response, and a poll message cannot be
    /// edited after it is created (`docs/poll.md`, Poll Endpoints), so this is a
    /// caller requirement.
    ///
    /// Reference: `docs/poll.md` (Poll Create Request Object) and
    /// `docs/webhook.md` (Edit Webhook Message).
    #[must_use]
    pub fn poll(mut self, poll: PollBuilder) -> Self {
        self.poll = Some(poll);
        self
    }

    /// Appends one file to upload with the edit.
    ///
    /// Files are appended to the message. API v10 additionally requires the
    /// `attachments` array to list every attachment that should remain after
    /// the edit, so a caller that replaces existing files must also list the
    /// retained ones with [`Self::keep_attachment`] (`docs/webhook.md`, Edit
    /// Webhook Message).
    ///
    /// Reference: `docs/reference.md` (Uploading Files) and `docs/webhook.md`
    /// (Edit Webhook Message).
    #[must_use]
    pub fn attachment(mut self, attachment: AttachmentBuilder) -> Self {
        self.attachments.push(attachment);
        self
    }

    /// Appends several files to upload with the edit.
    ///
    /// Reference: `docs/reference.md` (Uploading Files).
    #[must_use]
    pub fn attachments(mut self, attachments: impl IntoIterator<Item = AttachmentBuilder>) -> Self {
        self.attachments.extend(attachments);
        self
    }

    /// Keeps an existing attachment on the message when editing.
    ///
    /// Starting with API v10 the `attachments` array must contain all
    /// attachments that should be present after the edit, including retained
    /// ones; any attachment of the message that is not listed is removed
    /// (`docs/webhook.md`, Edit Webhook Message). Each retained attachment is
    /// listed with its documented id, and newly uploaded files keep their
    /// `files[n]` index as their id.
    ///
    /// Reference: `docs/message.md` (Attachment Request Structure) and
    /// `docs/webhook.md` (Edit Webhook Message).
    #[must_use]
    pub fn keep_attachment(mut self, attachment_id: AttachmentId) -> Self {
        self.kept_attachments.push(attachment_id);
        self
    }

    /// Keeps several existing attachments on the message when editing.
    ///
    /// Reference: `docs/webhook.md` (Edit Webhook Message).
    #[must_use]
    pub fn keep_attachments(
        mut self,
        attachment_ids: impl IntoIterator<Item = AttachmentId>,
    ) -> Self {
        self.kept_attachments.extend(attachment_ids);
        self
    }

    /// Returns the files to upload, in `files[n]` order.
    #[must_use]
    pub fn files(&self) -> &[AttachmentBuilder] {
        &self.attachments
    }

    /// Serialises the edit body:
    /// `{"content"?, "embeds"?, "components"?, "allowed_mentions"?,
    /// "attachments"?, "poll"?}`.
    ///
    /// When an `IS_COMPONENTS_V2` component is attached, the body sets the
    /// documented flag, emits only components (plus attachments) and resets the
    /// fields that do not apply to a V2 message, as the API requires
    /// (`docs/message.md`, Edit Message footnote).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();

        if self.requires_v2() {
            object.insert("components".to_owned(), Value::Array(self.v2_components()));
            object.insert("flags".to_owned(), Value::from(IS_COMPONENTS_V2_FLAG));
            object.insert("content".to_owned(), Value::Null);
            object.insert("embeds".to_owned(), Value::Array(Vec::new()));
            object.insert("poll".to_owned(), Value::Null);

            if let Some(allowed_mentions) = &self.allowed_mentions {
                object.insert("allowed_mentions".to_owned(), allowed_mentions.to_json());
            }

            if !self.attachments.is_empty() || !self.kept_attachments.is_empty() {
                let mut attachments = Vec::new();

                for attachment_id in &self.kept_attachments {
                    let mut kept = Map::new();
                    kept.insert("id".to_owned(), Value::String(attachment_id.to_string()));
                    attachments.push(Value::Object(kept));
                }

                if let Value::Array(new_attachments) =
                    attachment_request_metadata(&self.attachments)
                {
                    attachments.extend(new_attachments);
                }

                object.insert("attachments".to_owned(), Value::Array(attachments));
            }

            return Value::Object(object);
        }

        if let Some(content) = &self.content {
            object.insert(
                "content".to_owned(),
                Value::String(limits::truncate(content, limits::MESSAGE_CONTENT)),
            );
        }

        if !self.embeds.is_empty() {
            object.insert(
                "embeds".to_owned(),
                Value::Array(serialise_embeds(&self.embeds)),
            );
        }

        if !self.components.is_empty() {
            object.insert(
                "components".to_owned(),
                Value::Array(
                    self.components
                        .iter()
                        .take(limits::ACTION_ROWS)
                        .map(MessageComponentBuilder::to_json)
                        .collect(),
                ),
            );
        }

        if let Some(allowed_mentions) = &self.allowed_mentions {
            object.insert("allowed_mentions".to_owned(), allowed_mentions.to_json());
        }

        if !self.attachments.is_empty() || !self.kept_attachments.is_empty() {
            let mut attachments = Vec::new();

            for attachment_id in &self.kept_attachments {
                let mut kept = Map::new();
                kept.insert("id".to_owned(), Value::String(attachment_id.to_string()));
                attachments.push(Value::Object(kept));
            }

            if let Value::Array(new_attachments) = attachment_request_metadata(&self.attachments) {
                attachments.extend(new_attachments);
            }

            object.insert("attachments".to_owned(), Value::Array(attachments));
        }

        if let Some(poll) = &self.poll {
            object.insert("poll".to_owned(), poll.to_json());
        }

        Value::Object(object)
    }
}

impl InteractionEditBuilder {
    /// Returns true when any attached component is an `IS_COMPONENTS_V2`
    /// component.
    fn requires_v2(&self) -> bool {
        self.components
            .iter()
            .any(MessageComponentBuilder::requires_v2)
    }

    /// Serialises all attached components for an `IS_COMPONENTS_V2` edit.
    fn v2_components(&self) -> Vec<Value> {
        self.components
            .iter()
            .take(MAX_MESSAGE_COMPONENTS)
            .map(MessageComponentBuilder::to_json)
            .collect()
    }
}

/// Builds one action row of components (component type 1).
///
/// An action row holds up to five buttons *or* exactly one select
/// (`docs/components-reference.md`, Action Row). Calls that would violate that
/// rule are ignored: a select is not added to a row that already has children,
/// and buttons are not added after a select.
#[derive(Clone, Debug, Default)]
pub struct ActionRowBuilder {
    children: Vec<Value>,
    has_select: bool,
}

impl ActionRowBuilder {
    /// Creates an empty action row.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            children: Vec::new(),
            has_select: false,
        }
    }

    /// Appends one button; the sixth button in a row is ignored.
    #[must_use]
    pub fn button(mut self, button: ButtonBuilder) -> Self {
        if !self.has_select && self.children.len() < limits::BUTTONS_PER_ROW {
            self.children.push(Value::from(button));
        }
        self
    }

    /// Appends one text input, for the legacy modal action-row layout.
    ///
    /// Reference: `docs/components-reference.md` (Action Row) — an action row in
    /// a modal can hold up to 5 text inputs.
    #[must_use]
    pub fn text_input(mut self, input: TextInputBuilder) -> Self {
        if !self.has_select && self.children.len() < limits::TEXT_INPUTS_PER_ROW {
            self.children.push(Value::from(input));
        }
        self
    }

    /// Adds a single select component; ignored when the row already holds
    /// components.
    #[must_use]
    pub fn string_select(self, select: StringSelectBuilder) -> Self {
        self.select(Value::from(select))
    }

    /// Adds a single user select; ignored when the row already holds
    /// components.
    #[must_use]
    pub fn user_select(self, select: UserSelectBuilder) -> Self {
        self.select(Value::from(select))
    }

    /// Adds a single role select; ignored when the row already holds
    /// components.
    #[must_use]
    pub fn role_select(self, select: RoleSelectBuilder) -> Self {
        self.select(Value::from(select))
    }

    /// Adds a single mentionable select; ignored when the row already holds
    /// components.
    #[must_use]
    pub fn mentionable_select(self, select: MentionableSelectBuilder) -> Self {
        self.select(Value::from(select))
    }

    /// Adds a single channel select; ignored when the row already holds
    /// components.
    #[must_use]
    pub fn channel_select(self, select: ChannelSelectBuilder) -> Self {
        self.select(Value::from(select))
    }

    /// Adds one already-serialised select, when the row is still empty.
    fn select(mut self, select: Value) -> Self {
        if self.children.is_empty() {
            self.children.push(select);
            self.has_select = true;
        }
        self
    }

    /// Returns whether the row has no components.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Serialises the action row: `{"type": 1, "components": [...]}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        component_object(
            ComponentKind::ActionRow,
            [("components".to_owned(), Value::Array(self.children.clone()))],
        )
    }
}

/// Groups buttons into action rows of at most five.
///
/// Message components allow 5 action rows each holding up to 5 buttons; buttons
/// beyond that are ignored (`docs/components-reference.md`, Action Row).
fn button_action_rows(buttons: &[ButtonBuilder]) -> Vec<Value> {
    let mut rows = Vec::new();

    for chunk in buttons.chunks(limits::BUTTONS_PER_ROW) {
        if rows.len() == limits::ACTION_ROWS {
            break;
        }

        let mut row = ActionRowBuilder::new();
        for button in chunk {
            row = row.button(button.clone());
        }
        rows.push(row.to_json());
    }

    rows
}

/// Builds the JSON object for a component: the `type` plus the given fields.
fn component_object(
    kind: ComponentKind,
    fields: impl IntoIterator<Item = (String, Value)>,
) -> Value {
    let mut object = Map::new();
    object.insert("type".to_owned(), Value::from(kind.as_i64()));
    object.extend(fields);

    Value::Object(object)
}

/// Inserts a string field, clipped to `max` characters, when it is set.
fn insert_string(object: &mut Map<String, Value>, key: &str, value: Option<&String>, max: usize) {
    if let Some(value) = value {
        object.insert(key.to_owned(), Value::String(limits::truncate(value, max)));
    }
}

/// Inserts an integer field when it is set.
fn insert_number(object: &mut Map<String, Value>, key: &str, value: Option<u32>) {
    if let Some(value) = value {
        object.insert(key.to_owned(), Value::from(value));
    }
}

/// Inserts a boolean field when it is set.
fn insert_bool(object: &mut Map<String, Value>, key: &str, value: Option<bool>) {
    if let Some(value) = value {
        object.insert(key.to_owned(), Value::Bool(value));
    }
}

/// Applies the documented `min_values`/`max_values` rules shared by every
/// select component.
///
/// `min_values` and `max_values` have a documented maximum of 25, and
/// `min_values` "must be either omitted or at least 1 if `required` is omitted
/// or `true`" (`docs/components-reference.md`, String Select Structure; the
/// auto-populated selects document the same fields). Values outside the
/// documented range are clipped, like the rest of the builder layer.
fn select_value_bounds(
    min_values: Option<u32>,
    max_values: Option<u32>,
    required: Option<bool>,
) -> (Option<u32>, Option<u32>) {
    let min_values = min_values.map(|value| {
        let clipped = value.min(limits::SELECT_MAX_VALUES);
        if required.unwrap_or(true) {
            clipped.max(1)
        } else {
            clipped
        }
    });
    let max_values = max_values.map(|value| value.min(limits::SELECT_MAX_VALUES));

    (min_values, max_values)
}

/// Inserts a serialised emoji field when it is set.
fn insert_emoji(object: &mut Map<String, Value>, emoji: Option<&ReactionType>) {
    if let Some(emoji) = emoji
        && let Ok(emoji) = serde_json::to_value(emoji)
    {
        object.insert("emoji".to_owned(), emoji);
    }
}

/// Builds a message button component (component type 2).
///
/// Button styles are listed in `docs/components-reference.md` (Button, Button
/// Styles): style 1 primary, 2 secondary, 3 success, 4 danger, 5 link and 6
/// premium. Link buttons carry a `url`; premium buttons carry a `sku_id`; the
/// other styles carry a `custom_id`.
#[derive(Clone, Debug)]
pub struct ButtonBuilder {
    style: i64,
    label: Option<String>,
    custom_id: Option<String>,
    url: Option<String>,
    sku_id: Option<String>,
    emoji: Option<ReactionType>,
    disabled: bool,
}

impl ButtonBuilder {
    /// Creates a link button (style 5) that opens `url`.
    ///
    /// Reference: `docs/components-reference.md` (Button) — a link button
    /// must have a `url` and cannot have a `custom_id`.
    #[must_use]
    pub fn link(url: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            style: 5,
            label: Some(label.into()),
            custom_id: None,
            url: Some(url.into()),
            sku_id: None,
            emoji: None,
            disabled: false,
        }
    }

    /// Creates a primary (blurple) button (style 1).
    #[must_use]
    pub fn primary(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::with_custom_id(1, custom_id, label)
    }

    /// Creates a secondary (grey) button (style 2).
    #[must_use]
    pub fn secondary(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::with_custom_id(2, custom_id, label)
    }

    /// Creates a success (green) button (style 3).
    #[must_use]
    pub fn success(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::with_custom_id(3, custom_id, label)
    }

    /// Creates a danger (red) button (style 4).
    #[must_use]
    pub fn danger(custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self::with_custom_id(4, custom_id, label)
    }

    /// Creates a premium (purchase) button (style 6) for `sku_id`.
    ///
    /// Reference: `docs/components-reference.md` (Button) — a premium button
    /// must contain a `sku_id` and cannot have a `custom_id`, `label`, `url` or
    /// `emoji`.
    #[must_use]
    pub fn premium(sku_id: impl Into<String>) -> Self {
        Self {
            style: 6,
            label: None,
            custom_id: None,
            url: None,
            sku_id: Some(sku_id.into()),
            emoji: None,
            disabled: false,
        }
    }

    /// Creates a button with a `custom_id` and the given style number.
    fn with_custom_id(style: i64, custom_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            style,
            label: Some(label.into()),
            custom_id: Some(custom_id.into()),
            url: None,
            sku_id: None,
            emoji: None,
            disabled: false,
        }
    }

    /// Sets the button's label (maximum 80 characters).
    #[must_use]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the button's emoji.
    ///
    /// The emoji is serialised with the component emoji object shape
    /// (`docs/components-reference.md`, Button Structure;
    /// `docs/emoji.md`, Emoji Object).
    #[must_use]
    pub fn emoji(mut self, emoji: impl Into<ReactionType>) -> Self {
        self.emoji = Some(emoji.into());
        self
    }

    /// Sets whether the button is disabled.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Serialises the button component.
    ///
    /// Shape: `{"type":2,"style":...,"label"?, "url"|"custom_id"|"sku_id"?,
    /// "emoji"?, "disabled"?}`; `disabled` is omitted when false, matching the
    /// API's default.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "type".to_owned(),
            Value::from(ComponentKind::Button.as_i64()),
        );
        object.insert("style".to_owned(), Value::from(self.style));
        insert_string(
            &mut object,
            "label",
            self.label.as_ref(),
            limits::BUTTON_LABEL,
        );
        insert_string(
            &mut object,
            "custom_id",
            self.custom_id.as_ref(),
            limits::CUSTOM_ID,
        );
        insert_string(&mut object, "url", self.url.as_ref(), limits::BUTTON_URL);
        if let Some(sku_id) = &self.sku_id {
            object.insert("sku_id".to_owned(), Value::String(sku_id.clone()));
        }
        insert_emoji(&mut object, self.emoji.as_ref());

        if self.disabled {
            object.insert("disabled".to_owned(), Value::Bool(true));
        }

        Value::Object(object)
    }
}

/// One choice of a [`StringSelectBuilder`].
///
/// Reference: `docs/components-reference.md` (String Select, Select Option
/// Structure).
#[derive(Clone, Debug, Default)]
pub struct SelectOptionBuilder {
    label: String,
    value: String,
    description: Option<String>,
    emoji: Option<ReactionType>,
    default: Option<bool>,
}

impl SelectOptionBuilder {
    /// Creates a select option with the given label and value.
    #[must_use]
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            description: None,
            emoji: None,
            default: None,
        }
    }

    /// Sets the option description (maximum 100 characters).
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the option emoji.
    #[must_use]
    pub fn emoji(mut self, emoji: impl Into<ReactionType>) -> Self {
        self.emoji = Some(emoji.into());
        self
    }

    /// Sets whether this option is selected by default.
    #[must_use]
    pub const fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    /// Serialises the select option object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "label".to_owned(),
            Value::String(limits::truncate(&self.label, limits::SELECT_OPTION_TEXT)),
        );
        object.insert(
            "value".to_owned(),
            Value::String(limits::truncate(&self.value, limits::SELECT_OPTION_TEXT)),
        );
        insert_string(
            &mut object,
            "description",
            self.description.as_ref(),
            limits::SELECT_OPTION_TEXT,
        );
        insert_emoji(&mut object, self.emoji.as_ref());
        insert_bool(&mut object, "default", self.default);

        Value::Object(object)
    }
}

/// Builds a string select (component type 3, `docs/components-reference.md`,
/// String Select).
///
/// A string select can be placed in a message action row or in a modal label.
/// At most 25 options are serialised.
#[derive(Clone, Debug, Default)]
pub struct StringSelectBuilder {
    custom_id: String,
    options: Vec<SelectOptionBuilder>,
    placeholder: Option<String>,
    min_values: Option<u32>,
    max_values: Option<u32>,
    required: Option<bool>,
    disabled: Option<bool>,
}

impl StringSelectBuilder {
    /// Creates an empty string select with the given `custom_id`.
    #[must_use]
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            options: Vec::new(),
            placeholder: None,
            min_values: None,
            max_values: None,
            required: None,
            disabled: None,
        }
    }

    /// Appends one option.
    #[must_use]
    pub fn option(mut self, option: SelectOptionBuilder) -> Self {
        self.options.push(option);
        self
    }

    /// Appends options.
    #[must_use]
    pub fn options(mut self, options: impl IntoIterator<Item = SelectOptionBuilder>) -> Self {
        self.options.extend(options);
        self
    }

    /// Sets the placeholder shown when nothing is selected (maximum 150
    /// characters).
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Sets the minimum number of choices.
    #[must_use]
    pub const fn min_values(mut self, min_values: u32) -> Self {
        self.min_values = Some(min_values);
        self
    }

    /// Sets the maximum number of choices.
    #[must_use]
    pub const fn max_values(mut self, max_values: u32) -> Self {
        self.max_values = Some(max_values);
        self
    }

    /// Whether the select is required in a modal (defaults to true).
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Whether the select is disabled in a message.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = Some(disabled);
        self
    }

    /// Serialises the string select component.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let options = self
            .options
            .iter()
            .take(limits::SELECT_OPTIONS)
            .map(SelectOptionBuilder::to_json)
            .collect();

        let mut object = Map::new();
        object.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );
        object.insert("options".to_owned(), Value::Array(options));
        insert_string(
            &mut object,
            "placeholder",
            self.placeholder.as_ref(),
            limits::SELECT_PLACEHOLDER,
        );
        let (min_values, max_values) =
            select_value_bounds(self.min_values, self.max_values, self.required);
        insert_number(&mut object, "min_values", min_values);
        insert_number(&mut object, "max_values", max_values);
        insert_bool(&mut object, "required", self.required);
        insert_bool(&mut object, "disabled", self.disabled);

        component_object(ComponentKind::StringSelect, object)
    }
}

/// The configuration shared by the auto-populated select builders.
#[derive(Clone, Debug, Default)]
struct AutoSelect {
    custom_id: String,
    placeholder: Option<String>,
    min_values: Option<u32>,
    max_values: Option<u32>,
    required: Option<bool>,
    disabled: Option<bool>,
}

impl AutoSelect {
    /// Creates the shared configuration for `custom_id`.
    fn new(custom_id: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            ..Self::default()
        }
    }

    /// Serialises the fields shared by every auto-populated select.
    fn to_json(&self, kind: ComponentKind) -> Value {
        let mut object = Map::new();
        object.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );
        insert_string(
            &mut object,
            "placeholder",
            self.placeholder.as_ref(),
            limits::SELECT_PLACEHOLDER,
        );
        let (min_values, max_values) =
            select_value_bounds(self.min_values, self.max_values, self.required);
        insert_number(&mut object, "min_values", min_values);
        insert_number(&mut object, "max_values", max_values);
        insert_bool(&mut object, "required", self.required);
        insert_bool(&mut object, "disabled", self.disabled);

        component_object(kind, object)
    }
}

/// Generates one auto-populated select builder (component types 5, 6, 7 and
/// 8) from the fields it shares with the others.
macro_rules! auto_populated_select_builder {
    ($(#[$struct_docs:meta])* $name:ident, $kind:expr) => {
        $(#[$struct_docs])*
        #[derive(Clone, Debug, Default)]
        pub struct $name {
            select: AutoSelect,
        }

        impl $name {
            /// Creates an empty select with the given `custom_id`.
            #[must_use]
            pub fn new(custom_id: impl Into<String>) -> Self {
                Self {
                    select: AutoSelect::new(custom_id),
                }
            }

            /// Sets the placeholder shown when nothing is selected (maximum
            /// 150 characters).
            #[must_use]
            pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
                self.select.placeholder = Some(placeholder.into());
                self
            }

            /// Sets the minimum number of choices.
            #[must_use]
            pub const fn min_values(mut self, min_values: u32) -> Self {
                self.select.min_values = Some(min_values);
                self
            }

            /// Sets the maximum number of choices.
            #[must_use]
            pub const fn max_values(mut self, max_values: u32) -> Self {
                self.select.max_values = Some(max_values);
                self
            }

            /// Whether the select is required in a modal (defaults to true).
            #[must_use]
            pub const fn required(mut self, required: bool) -> Self {
                self.select.required = Some(required);
                self
            }

            /// Whether the select is disabled in a message.
            #[must_use]
            pub const fn disabled(mut self, disabled: bool) -> Self {
                self.select.disabled = Some(disabled);
                self
            }

            /// Serialises the select component.
            #[must_use]
            pub fn to_json(&self) -> Value {
                self.select.to_json($kind)
            }
        }
    };
}

auto_populated_select_builder!(
    /// Builds a user select (component type 5,
    /// `docs/components-reference.md`, User Select).
    UserSelectBuilder,
    ComponentKind::UserSelect
);

auto_populated_select_builder!(
    /// Builds a role select (component type 6,
    /// `docs/components-reference.md`, Role Select).
    RoleSelectBuilder,
    ComponentKind::RoleSelect
);

auto_populated_select_builder!(
    /// Builds a mentionable select (component type 7,
    /// `docs/components-reference.md`, Mentionable Select).
    MentionableSelectBuilder,
    ComponentKind::MentionableSelect
);

/// Builds a channel select (component type 8,
/// `docs/components-reference.md`, Channel Select).
#[derive(Clone, Debug, Default)]
pub struct ChannelSelectBuilder {
    select: AutoSelect,
    channel_types: Vec<crate::model::channel::ChannelKind>,
}

impl ChannelSelectBuilder {
    /// Creates an empty channel select with the given `custom_id`.
    #[must_use]
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            select: AutoSelect::new(custom_id),
            channel_types: Vec::new(),
        }
    }

    /// Restricts the select to the given channel kinds.
    #[must_use]
    pub fn channel_types(
        mut self,
        channel_types: impl IntoIterator<Item = crate::model::channel::ChannelKind>,
    ) -> Self {
        self.channel_types.extend(channel_types);
        self
    }

    /// Sets the placeholder shown when nothing is selected (maximum 150
    /// characters).
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.select.placeholder = Some(placeholder.into());
        self
    }

    /// Sets the minimum number of choices.
    #[must_use]
    pub const fn min_values(mut self, min_values: u32) -> Self {
        self.select.min_values = Some(min_values);
        self
    }

    /// Sets the maximum number of choices.
    #[must_use]
    pub const fn max_values(mut self, max_values: u32) -> Self {
        self.select.max_values = Some(max_values);
        self
    }

    /// Whether the select is required in a modal (defaults to true).
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.select.required = Some(required);
        self
    }

    /// Whether the select is disabled in a message.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.select.disabled = Some(disabled);
        self
    }

    /// Serialises the channel select component.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut value = self.select.to_json(ComponentKind::ChannelSelect);

        if !self.channel_types.is_empty()
            && let Value::Object(object) = &mut value
        {
            object.insert(
                "channel_types".to_owned(),
                Value::Array(
                    self.channel_types
                        .iter()
                        .map(|kind| Value::from(kind.as_i64()))
                        .collect(),
                ),
            );
        }

        value
    }
}

/// Builds a text input (component type 4, `docs/components-reference.md`, Text
/// Input).
///
/// Text inputs are only valid in modals, wrapped in a [`LabelBuilder`] (or, for
/// legacy modals, an [`ActionRowBuilder`]).
#[derive(Clone, Debug)]
pub struct TextInputBuilder {
    custom_id: String,
    style: TextInputStyle,
    placeholder: Option<String>,
    min_length: Option<u32>,
    max_length: Option<u32>,
    required: Option<bool>,
    value: Option<String>,
}

impl TextInputBuilder {
    /// Creates a single-line (short) text input.
    #[must_use]
    pub fn short(custom_id: impl Into<String>) -> Self {
        Self::new(custom_id, TextInputStyle::Short)
    }

    /// Creates a multi-line (paragraph) text input.
    #[must_use]
    pub fn paragraph(custom_id: impl Into<String>) -> Self {
        Self::new(custom_id, TextInputStyle::Paragraph)
    }

    /// Creates a text input with the given style.
    #[must_use]
    pub fn new(custom_id: impl Into<String>, style: TextInputStyle) -> Self {
        Self {
            custom_id: custom_id.into(),
            style,
            placeholder: None,
            min_length: None,
            max_length: None,
            required: None,
            value: None,
        }
    }

    /// Sets the placeholder shown while the input is empty (maximum 100
    /// characters).
    #[must_use]
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Sets the minimum input length (maximum 4000).
    #[must_use]
    pub const fn min_length(mut self, min_length: u32) -> Self {
        self.min_length = Some(min_length);
        self
    }

    /// Sets the maximum input length (1-4000).
    #[must_use]
    pub const fn max_length(mut self, max_length: u32) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// Whether the input must be filled (defaults to true).
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Sets the pre-filled value (maximum 4000 characters).
    #[must_use]
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Serialises the text input component.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );
        object.insert("style".to_owned(), Value::from(self.style.as_i64()));
        insert_string(
            &mut object,
            "placeholder",
            self.placeholder.as_ref(),
            limits::TEXT_INPUT_PLACEHOLDER,
        );
        insert_number(
            &mut object,
            "min_length",
            self.min_length
                .map(|value| value.min(limits::TEXT_INPUT_LENGTH)),
        );
        insert_number(
            &mut object,
            "max_length",
            self.max_length
                .map(|value| value.clamp(1, limits::TEXT_INPUT_LENGTH)),
        );
        insert_bool(&mut object, "required", self.required);
        if let Some(value) = &self.value {
            object.insert(
                "value".to_owned(),
                Value::String(limits::truncate(value, limits::TEXT_INPUT_LENGTH as usize)),
            );
        }

        component_object(ComponentKind::TextInput, object)
    }
}

/// Builds a label (component type 18, `docs/components-reference.md`, Label),
/// the modal container that wraps one interactive component with text.
///
/// The label text is at most 45 characters and the description at most 100.
#[derive(Clone, Debug, Default)]
pub struct LabelBuilder {
    label: String,
    description: Option<String>,
    component: Option<Value>,
}

impl LabelBuilder {
    /// Creates a label with the given text.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: None,
            component: None,
        }
    }

    /// Sets the description shown with the component (maximum 100 characters).
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Wraps a text input.
    #[must_use]
    pub fn text_input(self, input: TextInputBuilder) -> Self {
        self.wrap(Value::from(input))
    }

    /// Wraps a string select.
    #[must_use]
    pub fn string_select(self, select: StringSelectBuilder) -> Self {
        self.wrap(Value::from(select))
    }

    /// Wraps a user select.
    #[must_use]
    pub fn user_select(self, select: UserSelectBuilder) -> Self {
        self.wrap(Value::from(select))
    }

    /// Wraps a role select.
    #[must_use]
    pub fn role_select(self, select: RoleSelectBuilder) -> Self {
        self.wrap(Value::from(select))
    }

    /// Wraps a mentionable select.
    #[must_use]
    pub fn mentionable_select(self, select: MentionableSelectBuilder) -> Self {
        self.wrap(Value::from(select))
    }

    /// Wraps a channel select.
    #[must_use]
    pub fn channel_select(self, select: ChannelSelectBuilder) -> Self {
        self.wrap(Value::from(select))
    }

    /// Wraps any component builder accepted in a modal label, such as a file
    /// upload, radio group, checkbox group or checkbox.
    ///
    /// Reference: docs/components-reference.md (Label Interaction Response
    /// Child Components).
    #[must_use]
    pub fn component(self, component: impl Into<MessageComponentBuilder>) -> Self {
        self.wrap(Value::from(component.into()))
    }

    /// Wraps a file upload input.
    #[must_use]
    pub fn file_upload(self, input: super::component::FileUploadBuilder) -> Self {
        self.wrap(Value::from(input))
    }

    /// Wraps a radio group.
    #[must_use]
    pub fn radio_group(self, group: super::component::RadioGroupBuilder) -> Self {
        self.wrap(Value::from(group))
    }

    /// Wraps a checkbox group.
    #[must_use]
    pub fn checkbox_group(self, group: super::component::CheckboxGroupBuilder) -> Self {
        self.wrap(Value::from(group))
    }

    /// Wraps a single checkbox.
    #[must_use]
    pub fn checkbox(self, checkbox: super::component::CheckboxBuilder) -> Self {
        self.wrap(Value::from(checkbox))
    }

    /// Stores one serialised child component.
    fn wrap(mut self, component: Value) -> Self {
        self.component = Some(component);
        self
    }

    /// Serialises the label: `{"type": 18, "label", "description"?,
    /// "component"}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "label".to_owned(),
            Value::String(limits::truncate(&self.label, limits::LABEL_TEXT)),
        );
        insert_string(
            &mut object,
            "description",
            self.description.as_ref(),
            limits::LABEL_DESCRIPTION,
        );
        if let Some(component) = &self.component {
            object.insert("component".to_owned(), component.clone());
        }

        component_object(ComponentKind::Label, object)
    }
}

/// Implements `From<Builder> for serde_json::Value` for the component builders.
///
/// This is the documented serialisation of one component object, and it lets
/// the container builders consume their child builders by value.
macro_rules! component_builder_into_value {
    ($($builder:ty),+ $(,)?) => {
        $(
            impl From<$builder> for Value {
                fn from(builder: $builder) -> Self {
                    builder.to_json()
                }
            }
        )+
    };
}

component_builder_into_value!(
    ButtonBuilder,
    StringSelectBuilder,
    UserSelectBuilder,
    RoleSelectBuilder,
    MentionableSelectBuilder,
    ChannelSelectBuilder,
    TextInputBuilder,
);

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::builder::embed::EmbedBuilder;
    use crate::builder::message::AllowedMentionsBuilder;
    use crate::id::EmojiId;
    use crate::model::channel::ChannelKind;
    use crate::model::component::TextInputStyle;
    use crate::model::emoji::ReactionType;

    use super::{
        ActionRowBuilder, AutocompleteBuilder, AutocompleteChoice, ButtonBuilder,
        ChannelSelectBuilder, DeferBuilder, DeferUpdateBuilder, InteractionEditBuilder,
        InteractionResponse, LabelBuilder, MentionableSelectBuilder, ModalBuilder, ReplyBuilder,
        RoleSelectBuilder, SelectOptionBuilder, StringSelectBuilder, TextInputBuilder,
        UpdateMessageBuilder, UserSelectBuilder,
    };

    #[test]
    fn reply_with_ephemeral_flag_and_link_button_with_emoji() {
        let reply = ReplyBuilder::new()
            .content("Pong!")
            .ephemeral()
            .button(ButtonBuilder::link("https://example.com", "Docs").emoji("🏓"));

        assert_eq!(
            reply.to_json(),
            json!({
                "type": 4,
                "data": {
                    "content": "Pong!",
                    "components": [{
                        "type": 1,
                        "components": [{
                            "type": 2,
                            "style": 5,
                            "label": "Docs",
                            "url": "https://example.com",
                            "emoji": {"name": "🏓"}
                        }]
                    }],
                    "flags": 64
                }
            })
        );
    }

    #[test]
    fn custom_emoji_serialises_with_id_name_and_animated_only_when_true() {
        let custom = ReactionType::Custom {
            id: EmojiId::new(1_234_567_890),
            name: Some("wave".to_owned()),
            animated: false,
        };

        let button = ButtonBuilder::link("https://example.com", "Greet").emoji(custom);

        assert_eq!(
            button.to_json(),
            json!({
                "type": 2,
                "style": 5,
                "label": "Greet",
                "url": "https://example.com",
                "emoji": {"id": "1234567890", "name": "wave"}
            })
        );

        let animated = ReactionType::Custom {
            id: EmojiId::new(1_234_567_890),
            name: Some("wave".to_owned()),
            animated: true,
        };

        assert_eq!(
            ButtonBuilder::link("https://example.com", "Greet")
                .emoji(animated)
                .to_json()["emoji"],
            json!({"id": "1234567890", "name": "wave", "animated": true})
        );
    }

    #[test]
    fn non_link_button_styles_carry_a_custom_id() {
        let rows = ReplyBuilder::new()
            .button(ButtonBuilder::primary("a", "A"))
            .button(ButtonBuilder::secondary("b", "B"))
            .button(ButtonBuilder::success("c", "C"))
            .button(ButtonBuilder::danger("d", "D"))
            .to_json();

        assert_eq!(
            rows["data"]["components"][0],
            json!({
                "type": 1,
                "components": [
                    {"type": 2, "style": 1, "label": "A", "custom_id": "a"},
                    {"type": 2, "style": 2, "label": "B", "custom_id": "b"},
                    {"type": 2, "style": 3, "label": "C", "custom_id": "c"},
                    {"type": 2, "style": 4, "label": "D", "custom_id": "d"}
                ]
            })
        );
    }

    #[test]
    fn disabled_is_only_serialised_when_set() {
        assert_eq!(
            ButtonBuilder::primary("id", "Label").to_json(),
            json!({"type": 2, "style": 1, "label": "Label", "custom_id": "id"})
        );
        assert_eq!(
            ButtonBuilder::primary("id", "Label")
                .disabled(true)
                .to_json(),
            json!({"type": 2, "style": 1, "label": "Label", "custom_id": "id", "disabled": true})
        );
    }

    #[test]
    fn premium_buttons_carry_a_sku_id_and_no_label() {
        assert_eq!(
            ButtonBuilder::premium("1379220459203072050").to_json(),
            json!({"type": 2, "style": 6, "sku_id": "1379220459203072050"})
        );
    }

    #[test]
    fn buttons_are_grouped_into_rows_of_at_most_five() {
        let mut reply = ReplyBuilder::new();

        for index in 0..7 {
            reply = reply.button(ButtonBuilder::primary(
                format!("button{index}"),
                format!("Button {index}"),
            ));
        }

        let body = reply.to_json();
        let rows = body["data"]["components"].as_array().expect("rows");

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["components"].as_array().map(Vec::len), Some(5));
        assert_eq!(rows[1]["components"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn at_most_twenty_five_buttons_are_serialised() {
        let mut reply = ReplyBuilder::new();

        for index in 0..30 {
            reply = reply.button(ButtonBuilder::primary(
                format!("button{index}"),
                format!("Button {index}"),
            ));
        }

        let body = reply.to_json();
        let rows = body["data"]["components"].as_array().expect("rows");

        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4]["components"][4]["custom_id"], json!("button24"));
    }

    #[test]
    fn a_reply_can_carry_action_rows_of_components() {
        let body = ReplyBuilder::new()
            .content("Pick one")
            .component(
                ActionRowBuilder::new().string_select(
                    StringSelectBuilder::new("favorite")
                        .placeholder("Favorite?")
                        .option(SelectOptionBuilder::new("Ant", "ant").description("best")),
                ),
            )
            .to_json();

        assert_eq!(
            body["data"]["components"],
            json!([{
                "type": 1,
                "components": [{
                    "type": 3,
                    "custom_id": "favorite",
                    "options": [{"label": "Ant", "value": "ant", "description": "best"}],
                    "placeholder": "Favorite?"
                }]
            }])
        );
    }

    #[test]
    fn reply_components_share_the_five_row_limit_with_buttons() {
        let mut reply = ReplyBuilder::new();

        for index in 0..4 {
            reply = reply.button(ButtonBuilder::primary(
                format!("button{index}"),
                format!("Button {index}"),
            ));
        }
        for _ in 0..4 {
            reply = reply.component(
                ActionRowBuilder::new().button(ButtonBuilder::primary("extra", "Extra")),
            );
        }

        let rows = reply.to_json()["data"]["components"]
            .as_array()
            .cloned()
            .expect("rows");
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0]["components"][0]["custom_id"], json!("button0"));
        assert_eq!(rows[1]["components"][0]["custom_id"], json!("extra"));
        assert_eq!(rows[4]["components"][0]["custom_id"], json!("extra"));
    }

    #[test]
    fn an_action_row_holds_at_most_five_buttons() {
        let mut row = ActionRowBuilder::new();
        for index in 0..7 {
            row = row.button(ButtonBuilder::primary(format!("button{index}"), "Button"));
        }

        let json = row.to_json();
        let components = json["components"].as_array().expect("components");
        assert_eq!(components.len(), 5);
        assert_eq!(components[4]["custom_id"], json!("button4"));
    }

    #[test]
    fn an_action_row_holds_exactly_one_select() {
        let row = ActionRowBuilder::new()
            .string_select(StringSelectBuilder::new("first"))
            .string_select(StringSelectBuilder::new("second"));

        let json = row.to_json();
        let components = json["components"].as_array().expect("components");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0]["custom_id"], json!("first"));
    }

    #[test]
    fn a_select_is_not_added_to_a_row_that_already_has_buttons() {
        let row = ActionRowBuilder::new()
            .button(ButtonBuilder::primary("button", "Button"))
            .string_select(StringSelectBuilder::new("select"));

        let json = row.to_json();
        let components = json["components"].as_array().expect("components");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0]["type"], json!(2));
    }

    #[test]
    fn buttons_are_not_added_after_a_select() {
        let row = ActionRowBuilder::new()
            .string_select(StringSelectBuilder::new("select"))
            .button(ButtonBuilder::primary("button", "Button"));

        let json = row.to_json();
        let components = json["components"].as_array().expect("components");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0]["type"], json!(3));
    }

    #[test]
    fn string_select_serialises_options_and_value_bounds() {
        let json = StringSelectBuilder::new("favorite_bug")
            .placeholder("Favorite bug?")
            .min_values(1)
            .max_values(2)
            .option(
                SelectOptionBuilder::new("Ant", "ant")
                    .emoji("🐜")
                    .default(true),
            )
            .option(SelectOptionBuilder::new("Butterfly", "butterfly"))
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 3,
                "custom_id": "favorite_bug",
                "placeholder": "Favorite bug?",
                "min_values": 1,
                "max_values": 2,
                "options": [
                    {"label": "Ant", "value": "ant", "emoji": {"name": "🐜"}, "default": true},
                    {"label": "Butterfly", "value": "butterfly"}
                ]
            })
        );
    }

    #[test]
    fn string_select_clips_its_documented_limits() {
        let mut select = StringSelectBuilder::new("c".repeat(150)).placeholder("p".repeat(200));

        for index in 0..30 {
            select = select.option(SelectOptionBuilder::new(format!("label{index}"), "value"));
        }

        let json = select.max_values(99).to_json();

        assert_eq!(
            json["custom_id"]
                .as_str()
                .map(str::chars)
                .map(Iterator::count),
            Some(100)
        );
        assert_eq!(
            json["placeholder"]
                .as_str()
                .map(str::chars)
                .map(Iterator::count),
            Some(150)
        );
        assert_eq!(json["options"].as_array().map(Vec::len), Some(25));
        assert_eq!(json["max_values"], json!(25));
    }

    #[test]
    fn select_value_bounds_follow_the_documented_rules() {
        // docs/components-reference.md (String Select Structure):
        // `min_values` is "min 0 (see note), max 25" and "must be either
        // omitted or at least 1 if `required` is omitted or `true`"; the
        // auto-populated selects document the same fields.
        let json = StringSelectBuilder::new("select")
            .min_values(30)
            .max_values(30)
            .to_json();
        assert_eq!(json["min_values"], json!(25));
        assert_eq!(json["max_values"], json!(25));

        let json = StringSelectBuilder::new("select").min_values(0).to_json();
        assert_eq!(
            json["min_values"],
            json!(1),
            "required defaults to true, so min_values is at least 1"
        );

        let json = StringSelectBuilder::new("select")
            .min_values(0)
            .required(false)
            .to_json();
        assert_eq!(json["min_values"], json!(0));

        let json = UserSelectBuilder::new("users").min_values(0).to_json();
        assert_eq!(json["min_values"], json!(1));

        let json = UserSelectBuilder::new("users")
            .min_values(0)
            .required(false)
            .to_json();
        assert_eq!(json["min_values"], json!(0));
    }

    #[test]
    fn auto_populated_selects_serialise_their_component_types() {
        assert_eq!(
            UserSelectBuilder::new("user_select")
                .placeholder("Select a user")
                .max_values(5)
                .required(true)
                .to_json(),
            json!({
                "type": 5,
                "custom_id": "user_select",
                "placeholder": "Select a user",
                "max_values": 5,
                "required": true
            })
        );

        assert_eq!(
            RoleSelectBuilder::new("roles")
                .min_values(1)
                .max_values(3)
                .to_json(),
            json!({
                "type": 6,
                "custom_id": "roles",
                "min_values": 1,
                "max_values": 3
            })
        );

        assert_eq!(
            MentionableSelectBuilder::new("who")
                .disabled(true)
                .to_json(),
            json!({"type": 7, "custom_id": "who", "disabled": true})
        );
    }

    #[test]
    fn channel_select_serialises_the_documented_channel_types() {
        let json = ChannelSelectBuilder::new("notification_channel")
            .placeholder("Which text channel?")
            .channel_types([ChannelKind::GuildText, ChannelKind::GuildAnnouncement])
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 8,
                "custom_id": "notification_channel",
                "placeholder": "Which text channel?",
                "channel_types": [0, 5]
            })
        );
    }

    #[test]
    fn text_inputs_serialise_both_styles_and_their_limits() {
        assert_eq!(
            TextInputBuilder::short("name")
                .placeholder("Your name")
                .min_length(0)
                .max_length(100)
                .required(false)
                .value("prefilled")
                .to_json(),
            json!({
                "type": 4,
                "custom_id": "name",
                "style": 1,
                "placeholder": "Your name",
                "min_length": 0,
                "max_length": 100,
                "required": false,
                "value": "prefilled"
            })
        );

        assert_eq!(
            TextInputBuilder::paragraph("feedback").to_json(),
            json!({"type": 4, "custom_id": "feedback", "style": 2})
        );
    }

    #[test]
    fn text_input_lengths_are_clipped_to_the_documented_range() {
        let json = TextInputBuilder::short("c".repeat(150))
            .placeholder("p".repeat(150))
            .min_length(9_000)
            .max_length(0)
            .value("v".repeat(5_000))
            .to_json();

        assert_eq!(json["custom_id"], json!("c".repeat(100)));
        assert_eq!(json["placeholder"], json!("p".repeat(100)));
        assert_eq!(json["min_length"], json!(4_000));
        assert_eq!(json["max_length"], json!(1));
        assert_eq!(json["value"], json!("v".repeat(4_000)));
    }

    #[test]
    fn a_label_wraps_a_text_input() {
        let json = LabelBuilder::new("What did you find interesting?")
            .description("Please give us as much detail as possible!")
            .text_input(
                TextInputBuilder::paragraph("game_feedback")
                    .placeholder("Write your feedback here...")
                    .required(true),
            )
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 18,
                "label": "What did you find interesting?",
                "description": "Please give us as much detail as possible!",
                "component": {
                    "type": 4,
                    "custom_id": "game_feedback",
                    "style": 2,
                    "placeholder": "Write your feedback here...",
                    "required": true
                }
            })
        );
    }

    #[test]
    fn a_label_can_wrap_a_select() {
        let json = LabelBuilder::new("Pick one")
            .string_select(
                StringSelectBuilder::new("pick").option(SelectOptionBuilder::new("One", "one")),
            )
            .to_json();

        assert_eq!(json["component"]["type"], json!(3));
        assert_eq!(json["component"]["custom_id"], json!("pick"));
    }

    #[test]
    fn a_label_clips_its_documented_limits() {
        let json = LabelBuilder::new("l".repeat(100))
            .description("d".repeat(200))
            .text_input(TextInputBuilder::short("input"))
            .to_json();

        assert_eq!(json["label"], json!("l".repeat(45)));
        assert_eq!(json["description"], json!("d".repeat(100)));
    }

    #[test]
    fn empty_reply_has_an_empty_data_object() {
        assert_eq!(
            ReplyBuilder::new().to_json(),
            json!({"type": 4, "data": {}})
        );
    }

    #[test]
    fn reply_content_is_clipped_to_two_thousand_characters() {
        let body = ReplyBuilder::new().content("a".repeat(2_500)).to_json();
        let content = body["data"]["content"].as_str().expect("content");

        assert_eq!(content.chars().count(), 2000);
    }

    #[test]
    fn reply_embeds_serialise_like_message_embeds() {
        let body = ReplyBuilder::new()
            .embed(EmbedBuilder::new().title("Done").color(0x00_FF_00))
            .to_json();

        assert_eq!(
            body["data"]["embeds"],
            json!([{"title": "Done", "color": 65280}])
        );
    }

    #[test]
    fn reply_supports_tts_and_allowed_mentions() {
        let body = ReplyBuilder::new()
            .content("Hello <@123>")
            .tts(true)
            .allowed_mentions(AllowedMentionsBuilder::none())
            .to_json();

        assert_eq!(body["data"]["tts"], json!(true));
        assert_eq!(body["data"]["allowed_mentions"], json!({"parse": []}));
    }

    #[test]
    fn ephemeral_flag_controls_the_ephemeral_bit() {
        assert_eq!(
            ReplyBuilder::new().ephemeral_flag(true).to_json()["data"]["flags"],
            json!(64)
        );
        assert_eq!(
            ReplyBuilder::new()
                .ephemeral()
                .ephemeral_flag(false)
                .to_json()["data"]["flags"],
            Value::Null
        );
    }

    #[test]
    fn defer_payloads() {
        assert_eq!(
            DeferBuilder::new().to_json(),
            json!({"type": 5, "data": {}})
        );
        assert_eq!(
            DeferBuilder::new().ephemeral().to_json(),
            json!({"type": 5, "data": {"flags": 64}})
        );
        assert_eq!(
            DeferBuilder::new().ephemeral_flag(false).to_json(),
            json!({"type": 5, "data": {}})
        );
    }

    #[test]
    fn defer_update_payload_is_callback_type_six() {
        assert_eq!(DeferUpdateBuilder::new().to_json(), json!({"type": 6}));
    }

    #[test]
    fn update_message_payload_is_callback_type_seven() {
        let body = UpdateMessageBuilder::new()
            .content("Updated content")
            .embed(EmbedBuilder::new().title("Updated embed"))
            .component(ActionRowBuilder::new().button(ButtonBuilder::primary("again", "Again")))
            .to_json();

        assert_eq!(body["type"], json!(7));
        assert_eq!(body["data"]["content"], json!("Updated content"));
        assert_eq!(body["data"]["embeds"], json!([{"title": "Updated embed"}]));
        assert_eq!(
            body["data"]["components"][0]["components"][0]["custom_id"],
            json!("again")
        );
    }

    #[test]
    fn v2_replies_keep_the_allowed_mentions_object() {
        // `IS_COMPONENTS_V2` forbids `content`, `embeds`, `sticker_ids`,
        // `poll` and `shared_client_theme`, not `allowed_mentions`
        // (docs/message.md, Create Message footnote), and that object is what
        // filters the mentions inside text display components
        // (docs/components-reference.md, Text Display).
        let body = ReplyBuilder::new()
            .content("dropped for V2")
            .component(crate::builder::component::TextDisplayBuilder::new(
                "hello @everyone",
            ))
            .allowed_mentions(AllowedMentionsBuilder::none())
            .to_json();

        assert_eq!(body["data"]["flags"], json!(32_768));
        assert_eq!(body["data"]["allowed_mentions"], json!({"parse": []}));
        assert!(body["data"].get("content").is_none());
    }

    #[test]
    fn v2_update_messages_keep_the_allowed_mentions_object() {
        let body = UpdateMessageBuilder::new()
            .component(crate::builder::component::TextDisplayBuilder::new(
                "edited @everyone",
            ))
            .allowed_mentions(AllowedMentionsBuilder::none())
            .to_json();

        assert_eq!(body["type"], json!(7));
        assert_eq!(body["data"]["flags"], json!(32_768));
        assert_eq!(body["data"]["allowed_mentions"], json!({"parse": []}));
    }

    #[test]
    fn autocomplete_payload_serialises_choices_with_string_and_integer_values() {
        let body = AutocompleteBuilder::new()
            .choice("Animal: dog", "dog")
            .choice("Animal: cat", "cat")
            .choice("Count: 7", 7_i64)
            .to_json();

        assert_eq!(
            body,
            json!({
                "type": 8,
                "data": {
                    "choices": [
                        {"name": "Animal: dog", "value": "dog"},
                        {"name": "Animal: cat", "value": "cat"},
                        {"name": "Count: 7", "value": 7}
                    ]
                }
            })
        );
    }

    #[test]
    fn autocomplete_is_capped_at_twenty_five_choices() {
        let mut builder = AutocompleteBuilder::new();
        for index in 0..30 {
            builder = builder.choice(format!("choice {index}"), format!("value{index}"));
        }

        let body = builder.to_json();
        let choices = body["data"]["choices"].as_array().expect("choices");
        assert_eq!(choices.len(), 25);
        assert_eq!(choices[24]["name"], json!("choice 24"));
    }

    #[test]
    fn autocomplete_choice_names_are_clipped() {
        let body = AutocompleteBuilder::new()
            .choice("n".repeat(150), "v")
            .to_json();

        assert_eq!(
            body["data"]["choices"][0]["name"]
                .as_str()
                .map(str::chars)
                .map(Iterator::count),
            Some(100)
        );
    }

    #[test]
    fn autocomplete_choices_can_be_built_in_bulk() {
        let body = AutocompleteBuilder::new()
            .choices([
                AutocompleteChoice::new("one", "1"),
                AutocompleteChoice::new("two", "2"),
            ])
            .to_json();

        assert_eq!(body["data"]["choices"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn modal_payload_wraps_components_in_labels_and_rows() {
        let body = ModalBuilder::new("bug_modal", "Bug Report")
            .label(
                LabelBuilder::new("What's your favorite bug?").string_select(
                    StringSelectBuilder::new("bug_string_select")
                        .placeholder("Choose...")
                        .option(SelectOptionBuilder::new("Ant", "ant")),
                ),
            )
            .action_row(ActionRowBuilder::new().text_input(TextInputBuilder::short("legacy_input")))
            .to_json();

        assert_eq!(body["type"], json!(9));
        assert_eq!(body["data"]["custom_id"], json!("bug_modal"));
        assert_eq!(body["data"]["title"], json!("Bug Report"));
        assert_eq!(body["data"]["components"].as_array().map(Vec::len), Some(2));
        assert_eq!(body["data"]["components"][0]["type"], json!(18));
        assert_eq!(body["data"]["components"][0]["component"]["type"], json!(3));
        assert_eq!(body["data"]["components"][1]["type"], json!(1));
        assert_eq!(
            body["data"]["components"][1]["components"][0]["type"],
            json!(4)
        );
    }

    #[test]
    fn modal_components_are_capped_at_five_and_the_title_is_clipped() {
        let mut modal = ModalBuilder::new("c".repeat(150), "t".repeat(100));

        for index in 0..8 {
            modal = modal.label(
                LabelBuilder::new(format!("Label {index}"))
                    .text_input(TextInputBuilder::short(format!("input{index}"))),
            );
        }

        let body = modal.to_json();

        assert_eq!(
            body["data"]["custom_id"]
                .as_str()
                .map(str::chars)
                .map(Iterator::count),
            Some(100)
        );
        assert_eq!(body["data"]["title"], json!("t".repeat(45)));
        assert_eq!(body["data"]["components"].as_array().map(Vec::len), Some(5));
    }

    #[test]
    fn a_modal_can_hold_action_rows_with_several_text_inputs() {
        let mut row = ActionRowBuilder::new();
        for index in 0..5 {
            row = row.text_input(TextInputBuilder::short(format!("input{index}")));
        }

        let body = ModalBuilder::new("legacy", "Legacy modal")
            .action_row(row)
            .to_json();

        assert_eq!(
            body["data"]["components"][0]["components"]
                .as_array()
                .map(Vec::len),
            Some(5)
        );
    }

    #[test]
    fn edit_payload_with_content_and_embed() {
        let edit = InteractionEditBuilder::new()
            .content("Updated content")
            .embed(EmbedBuilder::new().title("Updated embed"));

        assert_eq!(
            edit.to_json(),
            json!({
                "content": "Updated content",
                "embeds": [{"title": "Updated embed"}]
            })
        );
    }

    #[test]
    fn edit_payload_can_replace_components() {
        let edit = InteractionEditBuilder::new()
            .content("Pick again")
            .component(ActionRowBuilder::new().user_select(UserSelectBuilder::new("user")));

        assert_eq!(
            edit.to_json(),
            json!({
                "content": "Pick again",
                "components": [{
                    "type": 1,
                    "components": [{"type": 5, "custom_id": "user"}]
                }]
            })
        );
    }

    #[test]
    fn edit_payload_with_no_fields_is_an_empty_object() {
        assert_eq!(InteractionEditBuilder::new().to_json(), json!({}));
    }

    #[test]
    fn interaction_response_wraps_the_builders() {
        let reply: InteractionResponse = ReplyBuilder::new().content("hi").into();
        assert_eq!(
            reply.to_json(),
            json!({"type": 4, "data": {"content": "hi"}})
        );

        let defer: InteractionResponse = DeferBuilder::new().ephemeral().into();
        assert_eq!(defer.to_json(), json!({"type": 5, "data": {"flags": 64}}));

        assert_eq!(InteractionResponse::Pong.to_json(), json!({"type": 1}));

        let defer_update: InteractionResponse = DeferUpdateBuilder::new().into();
        assert_eq!(defer_update.to_json(), json!({"type": 6}));

        let update: InteractionResponse = UpdateMessageBuilder::new().content("edited").into();
        assert_eq!(
            update.to_json(),
            json!({"type": 7, "data": {"content": "edited"}})
        );

        let autocomplete: InteractionResponse = AutocompleteBuilder::new().choice("a", "b").into();
        assert_eq!(autocomplete.to_json()["type"], json!(8));

        let modal: InteractionResponse = ModalBuilder::new("modal", "Title")
            .label(LabelBuilder::new("Name").text_input(TextInputBuilder::short("name")))
            .into();
        assert_eq!(modal.to_json()["type"], json!(9));
    }

    #[test]
    fn the_remaining_documented_callback_types_serialise() {
        assert_eq!(
            InteractionResponse::PremiumRequired.to_json(),
            json!({"type": 10})
        );
        assert_eq!(
            InteractionResponse::LaunchActivity.to_json(),
            json!({"type": 12})
        );
    }

    #[test]
    fn default_builders_match_new() {
        assert_eq!(
            ReplyBuilder::default().to_json(),
            ReplyBuilder::new().to_json()
        );
        assert_eq!(
            DeferBuilder::default().to_json(),
            DeferBuilder::new().to_json()
        );
        assert_eq!(
            DeferUpdateBuilder.to_json(),
            DeferUpdateBuilder::new().to_json()
        );
        assert_eq!(
            UpdateMessageBuilder::default().to_json(),
            UpdateMessageBuilder::new().to_json()
        );
        assert_eq!(
            AutocompleteBuilder::default().to_json(),
            AutocompleteBuilder::new().to_json()
        );
        assert_eq!(
            ModalBuilder::default().to_json(),
            ModalBuilder::new("", "").to_json()
        );
        assert_eq!(
            InteractionEditBuilder::default().to_json(),
            InteractionEditBuilder::new().to_json()
        );
        assert_eq!(
            TextInputBuilder::new("id", TextInputStyle::Short).to_json()["style"],
            json!(1)
        );
    }

    #[test]
    fn reply_carries_a_poll_inside_the_callback_data() {
        use crate::builder::poll::PollBuilder;

        let reply = ReplyBuilder::new()
            .content("Vote")
            .poll(PollBuilder::new("Deploy?").answer("Yes").answer("No"));

        let body = reply.to_json();

        assert_eq!(body["type"], json!(4));
        assert_eq!(
            body["data"]["poll"],
            json!({
                "question": {"text": "Deploy?"},
                "answers": [
                    {"poll_media": {"text": "Yes"}},
                    {"poll_media": {"text": "No"}}
                ]
            })
        );
        assert!(reply.files().is_empty());
    }

    #[test]
    fn reply_attachments_metadata_lives_inside_the_callback_data() {
        use crate::builder::attachment::AttachmentBuilder;

        let reply = ReplyBuilder::new().content("Here is a file").attachment(
            AttachmentBuilder::new("report.txt", b"data").with_description("The report"),
        );

        let body = reply.to_json();

        assert_eq!(
            body["data"]["attachments"],
            json!([{"id": 0, "filename": "report.txt", "description": "The report"}]),
            "interaction callback attachments belong under data (docs/receiving-and-responding.md)"
        );
        assert_eq!(reply.files().len(), 1);

        let response: InteractionResponse = reply.into();
        assert_eq!(response.files().len(), 1);
        assert_eq!(response.files()[0].filename(), "report.txt");
    }

    #[test]
    fn update_message_carries_poll_and_files() {
        use crate::builder::attachment::AttachmentBuilder;
        use crate::builder::poll::PollBuilder;

        let update = UpdateMessageBuilder::new()
            .content("Updated")
            .poll(PollBuilder::new("Keep going?").answer("Yes").answer("No"))
            .attachments([AttachmentBuilder::new("a.png", [9_u8])]);

        let body = update.to_json();

        assert_eq!(body["type"], json!(7));
        assert_eq!(
            body["data"]["poll"]["question"]["text"],
            json!("Keep going?")
        );
        assert_eq!(
            body["data"]["attachments"],
            json!([{"id": 0, "filename": "a.png"}])
        );

        let response: InteractionResponse = update.into();
        assert_eq!(response.files().len(), 1);
    }

    #[test]
    fn interaction_edit_carries_files_and_poll_at_the_top_level() {
        use crate::builder::attachment::AttachmentBuilder;
        use crate::builder::poll::PollBuilder;

        let edit = InteractionEditBuilder::new()
            .content("Edited")
            .attachment(AttachmentBuilder::new("edited.txt", b"x"))
            .poll(PollBuilder::new("Poll?").answer("A").answer("B"));

        let body = edit.to_json();

        assert_eq!(body["content"], json!("Edited"));
        assert_eq!(
            body["attachments"],
            json!([{"id": 0, "filename": "edited.txt"}]),
            "edit bodies carry attachments at the top level (docs/webhook.md, Edit Webhook Message)"
        );
        assert_eq!(body["poll"]["question"]["text"], json!("Poll?"));
        assert_eq!(edit.files().len(), 1);
    }

    #[test]
    fn interaction_edit_lists_retained_attachments_with_their_ids() {
        use crate::builder::attachment::AttachmentBuilder;
        use crate::id::AttachmentId;

        let edit = InteractionEditBuilder::new()
            .keep_attachment(AttachmentId::new(123))
            .keep_attachments([AttachmentId::new(456)])
            .attachment(AttachmentBuilder::new("new.txt", b"new"));

        let body = edit.to_json();

        // API v10 requires every attachment that should remain after the edit
        // to be listed: retained ones by snowflake id, new ones by files[n].
        assert_eq!(
            body["attachments"],
            json!([{"id": "123"}, {"id": "456"}, {"id": 0, "filename": "new.txt"}])
        );
        assert_eq!(
            edit.files().len(),
            1,
            "retained attachments are not uploaded again"
        );

        let kept_only = InteractionEditBuilder::new().keep_attachment(AttachmentId::new(7));
        assert_eq!(kept_only.to_json(), json!({"attachments": [{"id": "7"}]}));
        assert!(
            kept_only.files().is_empty(),
            "a kept-only edit is a plain JSON request"
        );
    }

    #[test]
    fn callbacks_without_message_content_upload_no_files() {
        assert!(InteractionResponse::Pong.files().is_empty());
        assert!(InteractionResponse::PremiumRequired.files().is_empty());
        assert!(InteractionResponse::LaunchActivity.files().is_empty());

        let defer: InteractionResponse = DeferBuilder::new().into();
        assert!(defer.files().is_empty());

        let modal: InteractionResponse = ModalBuilder::new("id", "Title").into();
        assert!(modal.files().is_empty());
    }

    #[test]
    fn interaction_response_to_payload_pairs_the_json_with_the_files() {
        use crate::builder::attachment::AttachmentBuilder;

        let response: InteractionResponse = ReplyBuilder::new()
            .content("with a file")
            .attachment(AttachmentBuilder::new("a.txt", b"a"))
            .into();

        let (payload, files) = response.to_payload();

        assert_eq!(payload, response.to_json());
        assert_eq!(files, response.files());
        assert_eq!(files.len(), 1);

        let (payload, files) = InteractionResponse::Pong.to_payload();
        assert_eq!(payload, json!({"type": 1}));
        assert!(files.is_empty());
    }
}
