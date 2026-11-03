//! Message and modal components: the component object and its documented
//! types.
//!
//! The component object is documented in `docs/components-reference.md`
//! (Component Object, plus one section per component type). This module types
//! every documented component:
//!
//! * layout components: [`ActionRow`] (`1`), [`Section`] (`9`), [`Separator`]
//!   (`14`), [`Container`] (`17`) and [`Label`] (`18`);
//! * content components: [`TextDisplay`] (`10`), [`Thumbnail`] (`11`),
//!   [`MediaGallery`] (`12`) and [`File`] (`13`);
//! * interactive components: [`Button`] (`2`), [`StringSelect`] (`3`),
//!   [`TextInput`] (`4`), [`UserSelect`] (`5`), [`RoleSelect`] (`6`),
//!   [`MentionableSelect`] (`7`), [`ChannelSelect`] (`8`), [`FileUpload`]
//!   (`19`), [`RadioGroup`] (`21`), [`CheckboxGroup`] (`22`) and [`Checkbox`]
//!   (`23`).
//!
//! The layout and content components above are the `IS_COMPONENTS_V2` set: a
//! message must set the `IS_COMPONENTS_V2` message flag (`1 << 15`) to use them
//! and can then carry up to 40 total components (`docs/message.md`, Message
//! Flags; `docs/components-reference.md`, Component Object).
//!
//! Deserialisation never fails on an unknown or malformed component: a value
//! whose `type` is not modelled, or whose body does not match the documented
//! shape, becomes [`Component::Other`] with the original JSON preserved
//! (`docs/components-reference.md`, Component Object; an unknown component type
//! must not break an inbound message).
//!
//! The interaction-response shapes of the interactive components
//! (`docs/components-reference.md`, each component's "Interaction Response
//! Structure") reuse the same types: the submitted [`TextInput::value`], the
//! [`RadioGroup::value`], the [`Checkbox::value`], and the
//! [`StringSelect::values`] / auto-populated select / [`FileUpload`] /
//! [`CheckboxGroup`] `values` arrays are filled in by modal submit payloads,
//! and [`crate::model::application::ModalSubmitData`] carries the nested
//! components. Response shapes that omit create-only fields (a submitted text
//! display has no `content`, a submitted radio group has no `options`)
//! deserialise through their documented defaults.

use std::fmt;

use serde::de::{DeserializeOwned, Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::id::{AttachmentId, GenericId};
use crate::model::emoji::ReactionType;

/// The documented integer `type` of a component object.
///
/// Reference: `docs/components-reference.md` (Component Types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComponentKind {
    /// Type `1`: an action row.
    ActionRow,
    /// Type `2`: a button.
    Button,
    /// Type `3`: a string select.
    StringSelect,
    /// Type `4`: a text input.
    TextInput,
    /// Type `5`: a user select.
    UserSelect,
    /// Type `6`: a role select.
    RoleSelect,
    /// Type `7`: a mentionable select.
    MentionableSelect,
    /// Type `8`: a channel select.
    ChannelSelect,
    /// Type `9`: a section.
    Section,
    /// Type `10`: a text display.
    TextDisplay,
    /// Type `11`: a thumbnail.
    Thumbnail,
    /// Type `12`: a media gallery.
    MediaGallery,
    /// Type `13`: a file.
    File,
    /// Type `14`: a separator.
    Separator,
    /// Type `17`: a container.
    Container,
    /// Type `18`: a label.
    Label,
    /// Type `19`: a file upload.
    FileUpload,
    /// Type `21`: a radio group.
    RadioGroup,
    /// Type `22`: a checkbox group.
    CheckboxGroup,
    /// Type `23`: a checkbox.
    Checkbox,
    /// A component type Discord added later.
    Other(i64),
}

impl ComponentKind {
    /// Maps the documented integer `type` value to a [`ComponentKind`].
    ///
    /// Unknown values are preserved as [`ComponentKind::Other`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::ActionRow,
            2 => Self::Button,
            3 => Self::StringSelect,
            4 => Self::TextInput,
            5 => Self::UserSelect,
            6 => Self::RoleSelect,
            7 => Self::MentionableSelect,
            8 => Self::ChannelSelect,
            9 => Self::Section,
            10 => Self::TextDisplay,
            11 => Self::Thumbnail,
            12 => Self::MediaGallery,
            13 => Self::File,
            14 => Self::Separator,
            17 => Self::Container,
            18 => Self::Label,
            19 => Self::FileUpload,
            21 => Self::RadioGroup,
            22 => Self::CheckboxGroup,
            23 => Self::Checkbox,
            other => Self::Other(other),
        }
    }

    /// Returns the documented integer `type` value.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::ActionRow => 1,
            Self::Button => 2,
            Self::StringSelect => 3,
            Self::TextInput => 4,
            Self::UserSelect => 5,
            Self::RoleSelect => 6,
            Self::MentionableSelect => 7,
            Self::ChannelSelect => 8,
            Self::Section => 9,
            Self::TextDisplay => 10,
            Self::Thumbnail => 11,
            Self::MediaGallery => 12,
            Self::File => 13,
            Self::Separator => 14,
            Self::Container => 17,
            Self::Label => 18,
            Self::FileUpload => 19,
            Self::RadioGroup => 21,
            Self::CheckboxGroup => 22,
            Self::Checkbox => 23,
            Self::Other(value) => value,
        }
    }
}

impl Serialize for ComponentKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for ComponentKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct KindVisitor;

        impl Visitor<'_> for KindVisitor {
            type Value = ComponentKind;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a component type integer")
            }

            fn visit_i64<E: DeError>(self, value: i64) -> Result<Self::Value, E> {
                Ok(ComponentKind::from_i64(value))
            }

            fn visit_u64<E: DeError>(self, value: u64) -> Result<Self::Value, E> {
                i64::try_from(value)
                    .map(ComponentKind::from_i64)
                    .map_err(|_| E::custom("component type out of range"))
            }
        }

        deserializer.deserialize_i64(KindVisitor)
    }
}

/// The style of a [`Button`].
///
/// Reference: `docs/components-reference.md` (Button, Button Styles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonStyle {
    /// Style `1`: primary (blurple).
    Primary,
    /// Style `2`: secondary (grey).
    Secondary,
    /// Style `3`: success (green).
    Success,
    /// Style `4`: danger (red).
    Danger,
    /// Style `5`: link, opens `url` and sends no interaction.
    Link,
    /// Style `6`: premium, purchases `sku_id` and sends no interaction.
    Premium,
    /// A style Discord added later.
    Other(i64),
}

impl ButtonStyle {
    /// Maps the documented integer style to a [`ButtonStyle`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::Primary,
            2 => Self::Secondary,
            3 => Self::Success,
            4 => Self::Danger,
            5 => Self::Link,
            6 => Self::Premium,
            other => Self::Other(other),
        }
    }

    /// Returns the documented integer style.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Primary => 1,
            Self::Secondary => 2,
            Self::Success => 3,
            Self::Danger => 4,
            Self::Link => 5,
            Self::Premium => 6,
            Self::Other(value) => value,
        }
    }
}

impl Serialize for ButtonStyle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for ButtonStyle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Unknown styles degrade instead of failing the enclosing component.
        let value = i64::deserialize(deserializer)?;

        Ok(Self::from_i64(value))
    }
}

/// The style of a [`TextInput`].
///
/// Reference: `docs/components-reference.md` (Text Input, Text Input Styles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextInputStyle {
    /// Style `1`: a single-line input.
    Short,
    /// Style `2`: a multi-line paragraph input.
    Paragraph,
    /// A style Discord added later.
    Other(i64),
}

impl TextInputStyle {
    /// Maps the documented integer style to a [`TextInputStyle`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::Short,
            2 => Self::Paragraph,
            other => Self::Other(other),
        }
    }

    /// Returns the documented integer style.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Short => 1,
            Self::Paragraph => 2,
            Self::Other(value) => value,
        }
    }
}

impl Serialize for TextInputStyle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for TextInputStyle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;

        Ok(Self::from_i64(value))
    }
}

/// The spacing of a [`Separator`].
///
/// Reference: `docs/components-reference.md` (Separator Structure) — `1` for
/// small padding and `2` for large padding; the default is `1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeparatorSpacing {
    /// Spacing `1`: small padding.
    Small,
    /// Spacing `2`: large padding.
    Large,
    /// A spacing value Discord added later.
    Other(i64),
}

impl SeparatorSpacing {
    /// Maps the documented integer spacing to a [`SeparatorSpacing`].
    #[must_use]
    pub const fn from_i64(value: i64) -> Self {
        match value {
            1 => Self::Small,
            2 => Self::Large,
            other => Self::Other(other),
        }
    }

    /// Returns the documented integer spacing.
    #[must_use]
    pub const fn as_i64(self) -> i64 {
        match self {
            Self::Small => 1,
            Self::Large => 2,
            Self::Other(value) => value,
        }
    }
}

impl Serialize for SeparatorSpacing {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.as_i64())
    }
}

impl<'de> Deserialize<'de> for SeparatorSpacing {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;

        Ok(Self::from_i64(value))
    }
}

/// One unfurled media item: a url (or `attachment://<filename>` reference) plus
/// the media metadata Discord fills in.
///
/// Only `url` is settable by developers when sending; every other field is
/// populated by Discord in the response (`docs/components-reference.md`,
/// Unfurled Media Item: "only the `url` field is settable by developers").
///
/// Reference: `docs/components-reference.md` (Unfurled Media Item, Unfurled
/// Media Item Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnfurledMediaItem {
    /// The media url: an arbitrary `http`/`https` url or an
    /// `attachment://<filename>` reference.
    pub url: String,
    /// The proxied url of the media item, filled in by Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    /// The height of the media item in pixels, filled in by Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    /// The width of the media item in pixels, filled in by Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// The Thumbhash placeholder for images and videos, filled in by Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The version of the Thumbhash placeholder, filled in by Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_version: Option<u32>,
    /// The media type of the content, filled in by Discord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// The Unfurled Media Item Flags bitfield, filled in by Discord.
    ///
    /// `IS_ANIMATED` is `1 << 0`; see [`UnfurledMediaItem::is_animated`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// The id of the uploaded attachment, present only for uploaded media.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment_id: Option<AttachmentId>,
}

impl UnfurledMediaItem {
    /// The documented `IS_ANIMATED` flag: `1 << 0`.
    ///
    /// Reference: `docs/components-reference.md` (Unfurled Media Item,
    /// Unfurled Media Item Flags).
    pub const FLAG_IS_ANIMATED: u64 = 1 << 0;

    /// Creates a media item from a url, the only settable field.
    ///
    /// Reference: `docs/components-reference.md` (Unfurled Media Item).
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            proxy_url: None,
            height: None,
            width: None,
            placeholder: None,
            placeholder_version: None,
            content_type: None,
            flags: None,
            attachment_id: None,
        }
    }

    /// Returns whether the documented `IS_ANIMATED` flag is set.
    #[must_use]
    pub fn is_animated(&self) -> bool {
        self.flags
            .is_some_and(|flags| flags & Self::FLAG_IS_ANIMATED != 0)
    }
}

/// One component object.
///
/// Reference: `docs/components-reference.md` (Component Object, Component
/// Types).
#[derive(Debug, Clone, PartialEq)]
pub enum Component {
    /// Type `1`: an action row.
    ActionRow(ActionRow),
    /// Type `2`: a button.
    Button(Button),
    /// Type `3`: a string select.
    StringSelect(StringSelect),
    /// Type `4`: a text input.
    TextInput(TextInput),
    /// Type `5`: a user select.
    UserSelect(UserSelect),
    /// Type `6`: a role select.
    RoleSelect(RoleSelect),
    /// Type `7`: a mentionable select.
    MentionableSelect(MentionableSelect),
    /// Type `8`: a channel select.
    ChannelSelect(ChannelSelect),
    /// Type `9`: a section.
    Section(Section),
    /// Type `10`: a text display.
    TextDisplay(TextDisplay),
    /// Type `11`: a thumbnail.
    Thumbnail(Thumbnail),
    /// Type `12`: a media gallery.
    MediaGallery(MediaGallery),
    /// Type `13`: a file.
    File(File),
    /// Type `14`: a separator.
    Separator(Separator),
    /// Type `17`: a container.
    Container(Container),
    /// Type `18`: a label.
    Label(Label),
    /// Type `19`: a file upload.
    FileUpload(FileUpload),
    /// Type `21`: a radio group.
    RadioGroup(RadioGroup),
    /// Type `22`: a checkbox group.
    CheckboxGroup(CheckboxGroup),
    /// Type `23`: a checkbox.
    Checkbox(Checkbox),
    /// A component type this crate does not model, kept as sent.
    ///
    /// Reference: `docs/components-reference.md` (Component Types) — a type
    /// Discord adds later, or a documented type whose body does not match the
    /// documented shape, is preserved raw instead of breaking an inbound
    /// message.
    Other(UnknownComponent),
}

impl Component {
    /// Returns the documented component type.
    #[must_use]
    pub fn kind(&self) -> ComponentKind {
        match self {
            Self::ActionRow(_) => ComponentKind::ActionRow,
            Self::Button(_) => ComponentKind::Button,
            Self::StringSelect(_) => ComponentKind::StringSelect,
            Self::TextInput(_) => ComponentKind::TextInput,
            Self::UserSelect(_) => ComponentKind::UserSelect,
            Self::RoleSelect(_) => ComponentKind::RoleSelect,
            Self::MentionableSelect(_) => ComponentKind::MentionableSelect,
            Self::ChannelSelect(_) => ComponentKind::ChannelSelect,
            Self::Section(_) => ComponentKind::Section,
            Self::TextDisplay(_) => ComponentKind::TextDisplay,
            Self::Thumbnail(_) => ComponentKind::Thumbnail,
            Self::MediaGallery(_) => ComponentKind::MediaGallery,
            Self::File(_) => ComponentKind::File,
            Self::Separator(_) => ComponentKind::Separator,
            Self::Container(_) => ComponentKind::Container,
            Self::Label(_) => ComponentKind::Label,
            Self::FileUpload(_) => ComponentKind::FileUpload,
            Self::RadioGroup(_) => ComponentKind::RadioGroup,
            Self::CheckboxGroup(_) => ComponentKind::CheckboxGroup,
            Self::Checkbox(_) => ComponentKind::Checkbox,
            Self::Other(unknown) => ComponentKind::from_i64(unknown.kind),
        }
    }

    /// Returns this component's `custom_id`, for the interactive types.
    ///
    /// Container and content types (action row, section, text display,
    /// thumbnail, media gallery, file, separator, container and label) return
    /// `None`; their children carry their own ids.
    ///
    /// Reference: `docs/components-reference.md` (Anatomy of a Component,
    /// Custom ID).
    #[must_use]
    pub fn custom_id(&self) -> Option<&str> {
        match self {
            Self::Button(button) => button.custom_id.as_deref(),
            Self::StringSelect(select) => Some(&select.custom_id),
            Self::TextInput(input) => Some(&input.custom_id),
            Self::UserSelect(select) => Some(&select.select.custom_id),
            Self::RoleSelect(select) => Some(&select.select.custom_id),
            Self::MentionableSelect(select) => Some(&select.select.custom_id),
            Self::ChannelSelect(select) => Some(&select.select.custom_id),
            Self::FileUpload(upload) => Some(&upload.custom_id),
            Self::RadioGroup(group) => Some(&group.custom_id),
            Self::CheckboxGroup(group) => Some(&group.custom_id),
            Self::Checkbox(checkbox) => Some(&checkbox.custom_id),
            Self::ActionRow(_)
            | Self::Section(_)
            | Self::TextDisplay(_)
            | Self::Thumbnail(_)
            | Self::MediaGallery(_)
            | Self::File(_)
            | Self::Separator(_)
            | Self::Container(_)
            | Self::Label(_)
            | Self::Other(_) => None,
        }
    }

    /// Returns the action row when this is type `1`.
    #[must_use]
    pub const fn as_action_row(&self) -> Option<&ActionRow> {
        match self {
            Self::ActionRow(row) => Some(row),
            _ => None,
        }
    }

    /// Returns the button when this is type `2`.
    #[must_use]
    pub const fn as_button(&self) -> Option<&Button> {
        match self {
            Self::Button(button) => Some(button),
            _ => None,
        }
    }

    /// Returns the string select when this is type `3`.
    #[must_use]
    pub const fn as_string_select(&self) -> Option<&StringSelect> {
        match self {
            Self::StringSelect(select) => Some(select),
            _ => None,
        }
    }

    /// Returns the text input when this is type `4`.
    #[must_use]
    pub const fn as_text_input(&self) -> Option<&TextInput> {
        match self {
            Self::TextInput(input) => Some(input),
            _ => None,
        }
    }

    /// Returns the user select when this is type `5`.
    #[must_use]
    pub const fn as_user_select(&self) -> Option<&UserSelect> {
        match self {
            Self::UserSelect(select) => Some(select),
            _ => None,
        }
    }

    /// Returns the role select when this is type `6`.
    #[must_use]
    pub const fn as_role_select(&self) -> Option<&RoleSelect> {
        match self {
            Self::RoleSelect(select) => Some(select),
            _ => None,
        }
    }

    /// Returns the mentionable select when this is type `7`.
    #[must_use]
    pub const fn as_mentionable_select(&self) -> Option<&MentionableSelect> {
        match self {
            Self::MentionableSelect(select) => Some(select),
            _ => None,
        }
    }

    /// Returns the channel select when this is type `8`.
    #[must_use]
    pub const fn as_channel_select(&self) -> Option<&ChannelSelect> {
        match self {
            Self::ChannelSelect(select) => Some(select),
            _ => None,
        }
    }

    /// Returns the section when this is type `9`.
    #[must_use]
    pub const fn as_section(&self) -> Option<&Section> {
        match self {
            Self::Section(section) => Some(section),
            _ => None,
        }
    }

    /// Returns the text display when this is type `10`.
    #[must_use]
    pub const fn as_text_display(&self) -> Option<&TextDisplay> {
        match self {
            Self::TextDisplay(display) => Some(display),
            _ => None,
        }
    }

    /// Returns the thumbnail when this is type `11`.
    #[must_use]
    pub const fn as_thumbnail(&self) -> Option<&Thumbnail> {
        match self {
            Self::Thumbnail(thumbnail) => Some(thumbnail),
            _ => None,
        }
    }

    /// Returns the media gallery when this is type `12`.
    #[must_use]
    pub const fn as_media_gallery(&self) -> Option<&MediaGallery> {
        match self {
            Self::MediaGallery(gallery) => Some(gallery),
            _ => None,
        }
    }

    /// Returns the file when this is type `13`.
    #[must_use]
    pub const fn as_file(&self) -> Option<&File> {
        match self {
            Self::File(file) => Some(file),
            _ => None,
        }
    }

    /// Returns the separator when this is type `14`.
    #[must_use]
    pub const fn as_separator(&self) -> Option<&Separator> {
        match self {
            Self::Separator(separator) => Some(separator),
            _ => None,
        }
    }

    /// Returns the container when this is type `17`.
    #[must_use]
    pub const fn as_container(&self) -> Option<&Container> {
        match self {
            Self::Container(container) => Some(container),
            _ => None,
        }
    }

    /// Returns the label when this is type `18`.
    #[must_use]
    pub const fn as_label(&self) -> Option<&Label> {
        match self {
            Self::Label(label) => Some(label),
            _ => None,
        }
    }

    /// Returns the file upload when this is type `19`.
    #[must_use]
    pub const fn as_file_upload(&self) -> Option<&FileUpload> {
        match self {
            Self::FileUpload(upload) => Some(upload),
            _ => None,
        }
    }

    /// Returns the radio group when this is type `21`.
    #[must_use]
    pub const fn as_radio_group(&self) -> Option<&RadioGroup> {
        match self {
            Self::RadioGroup(group) => Some(group),
            _ => None,
        }
    }

    /// Returns the checkbox group when this is type `22`.
    #[must_use]
    pub const fn as_checkbox_group(&self) -> Option<&CheckboxGroup> {
        match self {
            Self::CheckboxGroup(group) => Some(group),
            _ => None,
        }
    }

    /// Returns the checkbox when this is type `23`.
    #[must_use]
    pub const fn as_checkbox(&self) -> Option<&Checkbox> {
        match self {
            Self::Checkbox(checkbox) => Some(checkbox),
            _ => None,
        }
    }

    /// Builds a component from a wire value, degrading to
    /// [`Component::Other`] when the value is not a documented shape.
    fn from_value(value: Value) -> Self {
        let Some(kind) = value.get("type").and_then(Value::as_i64) else {
            return Self::Other(UnknownComponent {
                kind: 0,
                raw: value,
            });
        };

        match kind {
            1 | 9 | 14 | 17 | 18 => Self::parse_layout(kind, value),
            2..=8 | 19 | 21..=23 => Self::parse_interactive(kind, value),
            10..=13 => Self::parse_content(kind, value),
            _ => Self::Other(UnknownComponent { kind, raw: value }),
        }
    }

    /// Parses one of the documented layout components: [`ActionRow`] (`1`),
    /// [`Section`] (`9`), [`Separator`] (`14`), [`Container`] (`17`) or
    /// [`Label`] (`18`).
    ///
    /// Reference: `docs/components-reference.md` (Layout Components).
    fn parse_layout(kind: i64, value: Value) -> Self {
        match kind {
            1 => parse_known(kind, value, Self::ActionRow),
            9 => parse_known(kind, value, Self::Section),
            14 => parse_known(kind, value, Self::Separator),
            17 => parse_known(kind, value, Self::Container),
            18 => parse_known(kind, value, Self::Label),
            _ => Self::Other(UnknownComponent { kind, raw: value }),
        }
    }

    /// Parses one of the documented content components: [`TextDisplay`] (`10`),
    /// [`Thumbnail`] (`11`), [`MediaGallery`] (`12`) or [`File`] (`13`).
    ///
    /// Reference: `docs/components-reference.md` (Content Components).
    fn parse_content(kind: i64, value: Value) -> Self {
        match kind {
            10 => parse_known(kind, value, Self::TextDisplay),
            11 => parse_known(kind, value, Self::Thumbnail),
            12 => parse_known(kind, value, Self::MediaGallery),
            13 => parse_known(kind, value, Self::File),
            _ => Self::Other(UnknownComponent { kind, raw: value }),
        }
    }

    /// Parses one of the documented interactive components: [`Button`] (`2`),
    /// [`StringSelect`] (`3`), [`TextInput`] (`4`), the user/role/mentionable/
    /// channel selects (`5`-`8`), [`FileUpload`] (`19`), [`RadioGroup`] (`21`),
    /// [`CheckboxGroup`] (`22`) or [`Checkbox`] (`23`).
    ///
    /// Reference: `docs/components-reference.md` (Interactive Components).
    fn parse_interactive(kind: i64, value: Value) -> Self {
        match kind {
            2 => parse_known(kind, value, Self::Button),
            3 => parse_known(kind, value, Self::StringSelect),
            4 => parse_known(kind, value, Self::TextInput),
            5 => parse_known(kind, value, Self::UserSelect),
            6 => parse_known(kind, value, Self::RoleSelect),
            7 => parse_known(kind, value, Self::MentionableSelect),
            8 => parse_known(kind, value, Self::ChannelSelect),
            19 => parse_known(kind, value, Self::FileUpload),
            21 => parse_known(kind, value, Self::RadioGroup),
            22 => parse_known(kind, value, Self::CheckboxGroup),
            23 => parse_known(kind, value, Self::Checkbox),
            _ => Self::Other(UnknownComponent { kind, raw: value }),
        }
    }
}

/// Parses one known component type, falling back to
/// [`Component::Other`] when the body does not match the documented shape.
fn parse_known<T: DeserializeOwned>(
    kind: i64,
    value: Value,
    wrap: fn(T) -> Component,
) -> Component {
    match serde_json::from_value::<T>(value.clone()) {
        Ok(parsed) => wrap(parsed),
        Err(_) => Component::Other(UnknownComponent { kind, raw: value }),
    }
}

impl Serialize for Component {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::ActionRow(row) => row.serialize(serializer),
            Self::Button(button) => button.serialize(serializer),
            Self::StringSelect(select) => select.serialize(serializer),
            Self::TextInput(input) => input.serialize(serializer),
            Self::UserSelect(select) => select.serialize(serializer),
            Self::RoleSelect(select) => select.serialize(serializer),
            Self::MentionableSelect(select) => select.serialize(serializer),
            Self::ChannelSelect(select) => select.serialize(serializer),
            Self::Section(section) => section.serialize(serializer),
            Self::TextDisplay(display) => display.serialize(serializer),
            Self::Thumbnail(thumbnail) => thumbnail.serialize(serializer),
            Self::MediaGallery(gallery) => gallery.serialize(serializer),
            Self::File(file) => file.serialize(serializer),
            Self::Separator(separator) => separator.serialize(serializer),
            Self::Container(container) => container.serialize(serializer),
            Self::Label(label) => label.serialize(serializer),
            Self::FileUpload(upload) => upload.serialize(serializer),
            Self::RadioGroup(group) => group.serialize(serializer),
            Self::CheckboxGroup(group) => group.serialize(serializer),
            Self::Checkbox(checkbox) => checkbox.serialize(serializer),
            Self::Other(unknown) => unknown.raw.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Component {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;

        Ok(Self::from_value(value))
    }
}

/// A component type this crate does not model, kept as sent.
///
/// Reference: `docs/components-reference.md` (Component Types).
#[derive(Debug, Clone, PartialEq)]
pub struct UnknownComponent {
    /// The documented integer `type` of the component.
    pub kind: i64,
    /// The component object exactly as sent.
    pub raw: Value,
}

/// The component attached to a [`Section`] as its `accessory`.
///
/// The Component Reference lists [`Button`] and [`Thumbnail`] as the available
/// accessories and notes the field may be expanded to other components in the
/// future (`docs/components-reference.md`, Section, Section Accessory
/// Components); an accessory this crate does not model is preserved as
/// [`SectionAccessory::Other`] so a section never fails to deserialise.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionAccessory {
    /// A button accessory (component type `2`).
    Button(Button),
    /// A thumbnail accessory (component type `11`).
    Thumbnail(Thumbnail),
    /// An accessory component type this crate does not model, kept as sent.
    Other(UnknownComponent),
}

impl SectionAccessory {
    /// Returns the documented component type of the accessory.
    #[must_use]
    pub const fn kind(&self) -> ComponentKind {
        match self {
            Self::Button(_) => ComponentKind::Button,
            Self::Thumbnail(_) => ComponentKind::Thumbnail,
            Self::Other(unknown) => ComponentKind::from_i64(unknown.kind),
        }
    }

    /// Returns the button when the accessory is type `2`.
    #[must_use]
    pub const fn as_button(&self) -> Option<&Button> {
        match self {
            Self::Button(button) => Some(button),
            _ => None,
        }
    }

    /// Returns the thumbnail when the accessory is type `11`.
    #[must_use]
    pub const fn as_thumbnail(&self) -> Option<&Thumbnail> {
        match self {
            Self::Thumbnail(thumbnail) => Some(thumbnail),
            _ => None,
        }
    }

    /// Builds an accessory from a wire value, preserving unknown kinds raw.
    fn from_value(value: Value) -> Self {
        let kind = value.get("type").and_then(Value::as_i64).unwrap_or(0);

        match kind {
            2 => match serde_json::from_value::<Button>(value.clone()) {
                Ok(button) => Self::Button(button),
                Err(_) => Self::Other(UnknownComponent { kind, raw: value }),
            },
            11 => match serde_json::from_value::<Thumbnail>(value.clone()) {
                Ok(thumbnail) => Self::Thumbnail(thumbnail),
                Err(_) => Self::Other(UnknownComponent { kind, raw: value }),
            },
            _ => Self::Other(UnknownComponent { kind, raw: value }),
        }
    }
}

impl Serialize for SectionAccessory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Button(button) => button.serialize(serializer),
            Self::Thumbnail(thumbnail) => thumbnail.serialize(serializer),
            Self::Other(unknown) => unknown.raw.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for SectionAccessory {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;

        Ok(Self::from_value(value))
    }
}

/// Type `9`: a section, a top-level layout component that contextually
/// associates content with an accessory component.
///
/// The content is one to three child components ([`TextDisplay`]s today) and
/// the accessory is a [`Button`] or a [`Thumbnail`]
/// (`docs/components-reference.md`, Section, Section Child Components and
/// Section Accessory Components). Sections are available in messages that set
/// the `IS_COMPONENTS_V2` message flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    /// The component type, `9`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The content of the section; the Component Reference documents one to
    /// three child components and may expand the set in the future.
    #[serde(default)]
    pub components: Vec<Component>,
    /// The component contextually associated with the content.
    ///
    /// Documented as required when creating a section; kept optional so a
    /// payload that omits it still deserialises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessory: Option<SectionAccessory>,
}

/// Type `10`: a text display, a top-level content component holding markdown
/// text.
///
/// Reference: `docs/components-reference.md` (Text Display, Text Display
/// Structure and Text Display Interaction Response Structure). The interaction
/// response structure only carries `type` and `id`, so
/// [`TextDisplay::content`] defaults to the empty string in a submitted
/// component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextDisplay {
    /// The component type, `10`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The markdown text to display, similar to a message's `content`.
    ///
    /// The documented interaction response structure omits it, so it defaults
    /// to an empty string there.
    #[serde(default)]
    pub content: String,
}

/// Type `11`: a thumbnail, a content component showing visual media in a small
/// form-factor, used as a [`Section`] accessory.
///
/// Reference: `docs/components-reference.md` (Thumbnail, Thumbnail Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thumbnail {
    /// The component type, `11`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The media shown by the thumbnail.
    pub media: UnfurledMediaItem,
    /// Alt text for the media; at most 1024 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the thumbnail is a spoiler; defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
}

/// One item of a [`MediaGallery`].
///
/// Reference: `docs/components-reference.md` (Media Gallery, Media Gallery Item
/// Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaGalleryItem {
    /// The media displayed by the item.
    pub media: UnfurledMediaItem,
    /// Alt text for the media; at most 1024 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the media is a spoiler; defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
}

/// Type `12`: a media gallery, a top-level content component showing one to ten
/// media attachments.
///
/// Reference: `docs/components-reference.md` (Media Gallery, Media Gallery
/// Structure) — one to ten items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaGallery {
    /// The component type, `12`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The gallery items; the Component Reference documents one to ten.
    #[serde(default)]
    pub items: Vec<MediaGalleryItem>,
}

/// Type `13`: a file, a top-level content component displaying an uploaded
/// attachment.
///
/// The [`File::file`] media item only supports `attachment://<filename>`
/// references (`docs/components-reference.md`, File: "This unfurled media item
/// is unique in that it only supports attachment references").
///
/// Reference: `docs/components-reference.md` (File, File Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct File {
    /// The component type, `13`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The attached file as an `attachment://<filename>` media item.
    pub file: UnfurledMediaItem,
    /// Whether the media is a spoiler; defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
    /// The name of the file, provided by the API in the response and ignored on
    /// create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The size of the file in bytes, provided by the API in the response and
    /// ignored on create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// Type `14`: a separator, a top-level layout component adding vertical padding
/// between other components.
///
/// Reference: `docs/components-reference.md` (Separator, Separator Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Separator {
    /// The component type, `14`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// Whether a visual divider is displayed; defaults to `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub divider: Option<bool>,
    /// The separator padding; defaults to [`SeparatorSpacing::Small`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<SeparatorSpacing>,
}

/// Type `17`: a container, a top-level layout component that visually groups a
/// set of components and has an optional accent color bar.
///
/// Reference: `docs/components-reference.md` (Container, Container Structure
/// and Container Child Components).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Container {
    /// The component type, `17`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The encapsulated child components.
    #[serde(default)]
    pub components: Vec<Component>,
    /// The accent color as RGB from `0x000000` to `0xFFFFFF`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<u32>,
    /// Whether the container is a spoiler; defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spoiler: Option<bool>,
}

/// Type `19`: a file upload, an interactive modal component for uploading
/// files.
///
/// The documented response fields (`values`) are included too, so a file upload
/// submitted in a modal deserialises into the same type.
///
/// Reference: `docs/components-reference.md` (File Upload, File Upload
/// Structure and File Upload Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileUpload {
    /// The component type, `19`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The developer-defined identifier; 1-100 characters.
    pub custom_id: String,
    /// The minimum number of files; 0-10, defaults to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_values: Option<u32>,
    /// The maximum number of files; at most 10, defaults to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_values: Option<u32>,
    /// Whether files must be uploaded before submitting; defaults to `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// The accepted file types; at most 10 (`image`, `video`, `audio` or a
    /// dot-prefixed extension such as `.pdf`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_types: Vec<String>,
    /// The ids of the uploaded files in the resolved data, in an interaction
    /// response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

/// One option of a [`RadioGroup`] or a [`CheckboxGroup`].
///
/// The Component Reference gives the two option structures the same fields
/// (`docs/components-reference.md`, Radio Group Option Structure and Checkbox
/// Group Option Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupOption {
    /// The developer-defined value of the option; at most 100 characters.
    pub value: String,
    /// The user-facing label of the option; at most 100 characters.
    pub label: String,
    /// The optional description of the option; at most 100 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the option is selected by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
}

/// One option of a [`RadioGroup`].
pub type RadioGroupOption = GroupOption;

/// One option of a [`CheckboxGroup`].
pub type CheckboxGroupOption = GroupOption;

/// Type `21`: a radio group, an interactive modal component for selecting
/// exactly one of two to ten options.
///
/// The documented response field (`value`, nullable) is included too, so a
/// radio group submitted in a modal deserialises into the same type.
///
/// Reference: `docs/components-reference.md` (Radio Group, Radio Group
/// Structure and Radio Group Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadioGroup {
    /// The component type, `21`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The developer-defined identifier; 1-100 characters.
    pub custom_id: String,
    /// The options; the Component Reference documents a minimum of 2 and a
    /// maximum of 10.
    #[serde(default)]
    pub options: Vec<GroupOption>,
    /// Whether a selection is required to submit; defaults to `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// The value of the selected option in an interaction response, or `None`
    /// when no option is selected. The create structure has no `value` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Type `22`: a checkbox group, an interactive modal component for selecting
/// one or more of one to ten options.
///
/// The documented response field (`values`) is included too, so a checkbox
/// group submitted in a modal deserialises into the same type.
///
/// Reference: `docs/components-reference.md` (Checkbox Group, Checkbox Group
/// Structure and Checkbox Group Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckboxGroup {
    /// The component type, `22`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The developer-defined identifier; 1-100 characters.
    pub custom_id: String,
    /// The options; the Component Reference documents a minimum of 1 and a
    /// maximum of 10.
    #[serde(default)]
    pub options: Vec<GroupOption>,
    /// The minimum number of options that must be chosen; 0-10, defaults to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_values: Option<u32>,
    /// The maximum number of options that can be chosen; 1-10, defaults to the
    /// number of options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_values: Option<u32>,
    /// Whether selecting within the group is required; defaults to `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// The values of the selected options in an interaction response, or an
    /// empty array when none are selected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

/// Type `23`: a checkbox, an interactive modal component for a single yes/no
/// question.
///
/// The documented response field (`value`) is included too, so a checkbox
/// submitted in a modal deserialises into the same type.
///
/// Reference: `docs/components-reference.md` (Checkbox, Checkbox Structure and
/// Checkbox Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkbox {
    /// The component type, `23`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The developer-defined identifier; 1-100 characters.
    pub custom_id: String,
    /// Whether the checkbox is selected by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
    /// This is not part of the documented Checkbox structure — the Component
    /// Reference notes a checkbox cannot be required, and that a single-option
    /// [`CheckboxGroup`] should be used instead. It is accepted so a payload
    /// that sends it round-trips, and is omitted unless present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Whether the checkbox is checked in an interaction response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<bool>,
}

/// Type `1`: an action row, the container for message components.
///
/// Reference: `docs/components-reference.md` (Action Row). An action row holds
/// up to five buttons or exactly one select.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRow {
    /// The component type, `1`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The components in this row.
    #[serde(default)]
    pub components: Vec<Component>,
}

/// Type `2`: a button.
///
/// Reference: `docs/components-reference.md` (Button, Button Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Button {
    /// The component type, `2`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The button style.
    pub style: ButtonStyle,
    /// The text on the button; at most 80 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The partial emoji shown on the button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<ReactionType>,
    /// The developer-defined identifier; 1-100 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_id: Option<String>,
    /// The purchasable SKU id, for premium buttons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sku_id: Option<GenericId>,
    /// The url a link button opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Whether the button is disabled; defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

/// One choice of a [`StringSelect`].
///
/// Reference: `docs/components-reference.md` (String Select, Select Option
/// Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    /// The user-facing name; at most 100 characters.
    pub label: String,
    /// The developer-defined value; at most 100 characters.
    pub value: String,
    /// The additional description; at most 100 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The partial emoji shown on the option.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<ReactionType>,
    /// Whether the option is selected by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<bool>,
}

/// Type `3`: a string select.
///
/// The documented interaction-response fields (`values`) are included too, so
/// a select submitted in a modal deserialises into the same type.
///
/// Reference: `docs/components-reference.md` (String Select, String Select
/// Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StringSelect {
    /// The component type, `3`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The developer-defined identifier; 1-100 characters.
    pub custom_id: String,
    /// The choices in the select; at most 25.
    #[serde(default)]
    pub options: Vec<SelectOption>,
    /// The placeholder shown when nothing is selected; at most 150 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The minimum number of choices; 0-25.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_values: Option<u32>,
    /// The maximum number of choices; at most 25.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_values: Option<u32>,
    /// Whether the select must be answered in a modal; defaults to `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Whether the select is disabled in a message; defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// The text of the selected options, in an interaction response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

/// The style assumed when a text input payload omits `style` (the interaction
/// response structure does not include it).
const fn default_text_input_style() -> TextInputStyle {
    TextInputStyle::Short
}

/// Type `4`: a text input.
///
/// The documented interaction-response field (`value`) is included too, so a
/// text input submitted in a modal deserialises into the same type.
///
/// Reference: `docs/components-reference.md` (Text Input, Text Input
/// Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextInput {
    /// The component type, `4`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The developer-defined identifier; 1-100 characters.
    pub custom_id: String,
    /// The input style.
    ///
    /// Required when creating a text input; the documented interaction response
    /// structure omits it, so it defaults to
    /// [`TextInputStyle::Short`] there.
    #[serde(default = "default_text_input_style")]
    pub style: TextInputStyle,
    /// The deprecated input label.
    ///
    /// Reference: `docs/components-reference.md` (Text Input) — the `label`
    /// field is deprecated in favour of the [`Label`] component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The minimum input length; 0-4000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u32>,
    /// The maximum input length; 1-4000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
    /// Whether the input must be filled; defaults to `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// The pre-filled value, or the submitted value in an interaction
    /// response; at most 4000 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The placeholder shown while the input is empty; at most 100 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

/// One default value of an auto-populated select.
///
/// Reference: `docs/components-reference.md` (Select Default Value Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectDefaultValue {
    /// The id of the default user, role or channel.
    pub id: GenericId,
    /// What `id` names.
    #[serde(rename = "type")]
    pub kind: SelectDefaultValueKind,
}

/// The kind of a [`SelectDefaultValue`]: the documented `"user"`, `"role"` or
/// `"channel"` strings.
///
/// Reference: `docs/components-reference.md` (Select Default Value Structure).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SelectDefaultValueKind {
    /// `"user"`: the id is a user id.
    User,
    /// `"role"`: the id is a role id.
    Role,
    /// `"channel"`: the id is a channel id.
    Channel,
    /// A kind Discord added later.
    Other(String),
}

impl SelectDefaultValueKind {
    /// Maps the documented wire string to a [`SelectDefaultValueKind`].
    #[must_use]
    pub fn from_wire(value: &str) -> Self {
        match value {
            "user" => Self::User,
            "role" => Self::Role,
            "channel" => Self::Channel,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Returns the documented wire string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Role => "role",
            Self::Channel => "channel",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for SelectDefaultValueKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SelectDefaultValueKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;

        Ok(Self::from_wire(&value))
    }
}

/// The fields shared by the auto-populated selects (user, role, mentionable and
/// channel select).
///
/// Reference: `docs/components-reference.md` (User Select, Role Select,
/// Mentionable Select and Channel Select structures).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoPopulatedSelect {
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The developer-defined identifier; 1-100 characters.
    pub custom_id: String,
    /// The placeholder shown when nothing is selected; at most 150 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The default values of the select.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub default_values: Vec<SelectDefaultValue>,
    /// The minimum number of choices; 0-25.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_values: Option<u32>,
    /// The maximum number of choices; at most 25.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_values: Option<u32>,
    /// Whether the select must be answered in a modal; defaults to `true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Whether the select is disabled in a message; defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// The selected ids, in an interaction response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<String>,
}

/// Type `5`: a user select.
///
/// Reference: `docs/components-reference.md` (User Select, User Select
/// Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserSelect {
    /// The component type, `5`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The fields shared by the auto-populated selects.
    #[serde(flatten)]
    pub select: AutoPopulatedSelect,
}

/// Type `6`: a role select.
///
/// Reference: `docs/components-reference.md` (Role Select, Role Select
/// Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleSelect {
    /// The component type, `6`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The fields shared by the auto-populated selects.
    #[serde(flatten)]
    pub select: AutoPopulatedSelect,
}

/// Type `7`: a mentionable select (users and roles).
///
/// Reference: `docs/components-reference.md` (Mentionable Select, Mentionable
/// Select Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentionableSelect {
    /// The component type, `7`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The fields shared by the auto-populated selects.
    #[serde(flatten)]
    pub select: AutoPopulatedSelect,
}

/// Type `8`: a channel select.
///
/// Reference: `docs/components-reference.md` (Channel Select, Channel Select
/// Interaction Response Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelSelect {
    /// The component type, `8`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The fields shared by the auto-populated selects.
    #[serde(flatten)]
    pub select: AutoPopulatedSelect,
    /// The channel types to include in the select, as documented integer
    /// channel types.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channel_types: Vec<crate::model::channel::ChannelKind>,
}

/// Type `18`: a label, the modal container that wraps an interactive
/// component with a label and optional description.
///
/// Reference: `docs/components-reference.md` (Label, Label Structure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    /// The component type, `18`.
    #[serde(rename = "type")]
    pub kind: ComponentKind,
    /// The optional 32-bit component identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// The label text; at most 45 characters.
    ///
    /// This field is absent from the documented label interaction response
    /// structure, so it defaults to an empty string there.
    #[serde(default)]
    pub label: String,
    /// The optional description; at most 100 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The component inside the label.
    pub component: Box<Component>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn parse_component(value: Value) -> Component {
        serde_json::from_value(value).expect("a component fixture parses")
    }

    #[test]
    fn parses_an_action_row_with_buttons() {
        let component = parse_component(json!({
            "type": 1,
            "components": [
                {"type": 2, "style": 1, "label": "Accept", "custom_id": "click_yes"},
                {"type": 2, "style": 5, "label": "Learn More", "url": "https://example.com"},
                {"type": 2, "style": 4, "label": "Decline", "custom_id": "click_no"}
            ]
        }));

        assert_eq!(component.kind(), ComponentKind::ActionRow);
        let row = component.as_action_row().expect("action row");

        assert!(row.id.is_none());
        assert_eq!(row.components.len(), 3);
        assert_eq!(row.components[0].custom_id(), Some("click_yes"));
        assert_eq!(row.components[1].custom_id(), None);

        let Component::Button(link) = &row.components[1] else {
            panic!("expected a button");
        };
        assert_eq!(link.style, ButtonStyle::Link);
        assert_eq!(link.url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parses_every_documented_button_style() {
        for (value, style) in [
            (1, ButtonStyle::Primary),
            (2, ButtonStyle::Secondary),
            (3, ButtonStyle::Success),
            (4, ButtonStyle::Danger),
            (5, ButtonStyle::Link),
            (6, ButtonStyle::Premium),
        ] {
            let button = parse_component(json!({
                "type": 2,
                "style": value,
                "sku_id": "1234567890",
                "custom_id": "c",
                "url": "https://example.com",
                "label": "Label"
            }));

            let Component::Button(button) = button else {
                panic!("expected a button");
            };
            assert_eq!(button.style, style);
            assert_eq!(button.style.as_i64(), value);
        }
    }

    #[test]
    fn an_unknown_button_style_is_preserved() {
        let button = parse_component(json!({"type": 2, "style": 99, "custom_id": "c"}));

        let Component::Button(button) = button else {
            panic!("expected a button");
        };
        assert_eq!(button.style, ButtonStyle::Other(99));
        assert_eq!(button.style.as_i64(), 99);
    }

    #[test]
    fn parses_a_button_with_a_unicode_emoji_and_disabled_flag() {
        let button = parse_component(json!({
            "type": 2,
            "style": 2,
            "label": "Wave",
            "custom_id": "wave",
            "emoji": {"name": "👋"},
            "disabled": true
        }));

        let Component::Button(button) = button else {
            panic!("expected a button");
        };
        assert_eq!(button.emoji, Some(ReactionType::Unicode("👋".to_owned())));
        assert_eq!(button.disabled, Some(true));
    }

    #[test]
    fn parses_a_string_select_with_options() {
        let select = parse_component(json!({
            "type": 3,
            "id": 2,
            "custom_id": "favorite_bug",
            "placeholder": "Favorite bug?",
            "min_values": 1,
            "max_values": 2,
            "options": [
                {
                    "label": "Ant",
                    "value": "ant",
                    "description": "(best option)",
                    "emoji": {"name": "🐜"},
                    "default": true
                },
                {"label": "Butterfly", "value": "butterfly"}
            ]
        }));

        let Component::StringSelect(select) = select else {
            panic!("expected a string select");
        };
        assert_eq!(select.id, Some(2));
        assert_eq!(select.custom_id, "favorite_bug");
        assert_eq!(select.placeholder.as_deref(), Some("Favorite bug?"));
        assert_eq!(select.min_values, Some(1));
        assert_eq!(select.max_values, Some(2));
        assert_eq!(select.options.len(), 2);
        assert_eq!(select.options[0].label, "Ant");
        assert_eq!(select.options[0].default, Some(true));
        assert_eq!(
            select.options[0].emoji,
            Some(ReactionType::Unicode("🐜".to_owned()))
        );
        assert!(select.values.is_empty());
    }

    #[test]
    fn parses_a_string_select_interaction_response_with_values() {
        let select = parse_component(json!({
            "type": 3,
            "id": 2,
            "custom_id": "favorite_bug",
            "values": ["butterfly"]
        }));

        let Component::StringSelect(select) = select else {
            panic!("expected a string select");
        };
        assert_eq!(select.values, vec!["butterfly".to_owned()]);
        assert!(select.options.is_empty());
    }

    #[test]
    fn parses_a_short_text_input() {
        let input = parse_component(json!({
            "type": 4,
            "id": 3,
            "custom_id": "name",
            "style": 1,
            "min_length": 0,
            "max_length": 100,
            "required": false,
            "value": "prefilled",
            "placeholder": "Your name"
        }));

        let Component::TextInput(input) = input else {
            panic!("expected a text input");
        };
        assert_eq!(input.style, TextInputStyle::Short);
        assert_eq!(input.min_length, Some(0));
        assert_eq!(input.max_length, Some(100));
        assert_eq!(input.required, Some(false));
        assert_eq!(input.value.as_deref(), Some("prefilled"));
        assert_eq!(input.placeholder.as_deref(), Some("Your name"));
    }

    #[test]
    fn parses_a_paragraph_text_input_interaction_response() {
        let input = parse_component(json!({
            "type": 4,
            "custom_id": "feedback",
            "style": 2,
            "value": "The acceleration feels better"
        }));

        let Component::TextInput(input) = input else {
            panic!("expected a text input");
        };
        assert_eq!(input.style, TextInputStyle::Paragraph);
        assert_eq!(
            input.value.as_deref(),
            Some("The acceleration feels better")
        );
    }

    #[test]
    fn parses_the_auto_populated_selects() {
        let user = parse_component(json!({
            "type": 5,
            "custom_id": "user_select",
            "placeholder": "Select a user",
            "max_values": 5,
            "required": true,
            "default_values": [{"id": "1111111111111111111", "type": "user"}],
            "values": ["1111111111111111111"]
        }));
        let Component::UserSelect(user) = user else {
            panic!("expected a user select");
        };
        assert_eq!(user.select.custom_id, "user_select");
        assert_eq!(user.select.values, vec!["1111111111111111111".to_owned()]);
        assert_eq!(user.select.default_values.len(), 1);
        assert_eq!(
            user.select.default_values[0].kind,
            SelectDefaultValueKind::User
        );

        let role = parse_component(json!({"type": 6, "custom_id": "roles"}));
        assert_eq!(role.kind(), ComponentKind::RoleSelect);
        assert_eq!(role.custom_id(), Some("roles"));

        let mentionable = parse_component(json!({"type": 7, "custom_id": "who"}));
        assert_eq!(mentionable.kind(), ComponentKind::MentionableSelect);

        let channel = parse_component(json!({
            "type": 8,
            "custom_id": "notification_channel",
            "channel_types": [0, 5],
            "values": ["333333333333333333"]
        }));
        let Component::ChannelSelect(channel) = channel else {
            panic!("expected a channel select");
        };
        assert_eq!(
            channel.channel_types,
            vec![
                crate::model::channel::ChannelKind::GuildText,
                crate::model::channel::ChannelKind::GuildAnnouncement
            ]
        );
        assert_eq!(channel.select.values, vec!["333333333333333333".to_owned()]);
    }

    #[test]
    fn parses_a_label_wrapping_a_text_input() {
        let label = parse_component(json!({
            "type": 18,
            "id": 1,
            "label": "What did you find interesting?",
            "description": "Give us as much detail as possible",
            "component": {
                "type": 4,
                "id": 2,
                "custom_id": "game_feedback",
                "style": 2,
                "required": true
            }
        }));

        let Component::Label(label) = label else {
            panic!("expected a label");
        };
        assert_eq!(label.label, "What did you find interesting?");
        assert_eq!(
            label.description.as_deref(),
            Some("Give us as much detail as possible")
        );
        assert_eq!(label.component.custom_id(), Some("game_feedback"));
        assert_eq!(label.component.kind(), ComponentKind::TextInput);
    }

    #[test]
    fn parses_a_section_with_a_thumbnail_accessory() {
        // The documented Section example (`docs/components-reference.md`,
        // Section): three text displays and a thumbnail accessory.
        let section = parse_component(json!({
            "type": 9,
            "components": [
                {"type": 10, "content": "# Real Game v7.3"},
                {"type": 10, "content": "Hope you're excited, the update is finally here!"},
                {"type": 10, "content": "-# That last one wasn't real"}
            ],
            "accessory": {
                "type": 11,
                "media": {"url": "https://websitewithopensourceimages/gamepreview.webp"}
            }
        }));

        let Component::Section(section) = section else {
            panic!("expected a section");
        };
        assert_eq!(section.kind, ComponentKind::Section);
        assert!(section.id.is_none());
        assert_eq!(section.components.len(), 3);
        assert_eq!(section.components[0].kind(), ComponentKind::TextDisplay);
        assert_eq!(
            section.components[0]
                .as_text_display()
                .map(|display| display.content.as_str()),
            Some("# Real Game v7.3")
        );

        let accessory = section.accessory.expect("thumbnail accessory");
        assert_eq!(accessory.kind(), ComponentKind::Thumbnail);
        let thumbnail = accessory.as_thumbnail().expect("a thumbnail");
        assert_eq!(
            thumbnail.media.url,
            "https://websitewithopensourceimages/gamepreview.webp"
        );
        assert_eq!(thumbnail.description, None);
        assert_eq!(thumbnail.spoiler, None);
    }

    #[test]
    fn parses_a_section_with_a_button_accessory() {
        let component = parse_component(json!({
            "type": 9,
            "components": [{"type": 10, "content": "Open the page"}],
            "accessory": {
                "type": 2,
                "style": 5,
                "label": "Open",
                "url": "https://watchanimeattheoffice.com/"
            }
        }));

        let Component::Section(section) = component else {
            panic!("expected a section");
        };
        let accessory = section.accessory.expect("button accessory");
        assert_eq!(accessory.kind(), ComponentKind::Button);
        assert_eq!(
            accessory.as_button().map(|button| button.style),
            Some(ButtonStyle::Link)
        );
        assert!(accessory.as_thumbnail().is_none());
    }

    #[test]
    fn unknown_section_accessory_types_are_preserved() {
        let component = parse_component(json!({
            "type": 9,
            "components": [],
            "accessory": {"type": 24, "future_field": true}
        }));

        let Component::Section(section) = component else {
            panic!("expected a section");
        };
        let SectionAccessory::Other(unknown) = section.accessory.expect("accessory") else {
            panic!("expected the raw accessory fallback");
        };
        assert_eq!(unknown.kind, 24);
        assert_eq!(unknown.raw["future_field"], json!(true));
    }

    #[test]
    fn parses_a_text_display_and_its_interaction_response() {
        let display = parse_component(json!({
            "type": 10,
            "id": 4,
            "content": "# This is a Text Display"
        }));

        let Component::TextDisplay(display) = display else {
            panic!("expected a text display");
        };
        assert_eq!(display.content, "# This is a Text Display");
        assert_eq!(display.id, Some(4));

        // The documented Text Display interaction response carries only `type`
        // and `id`.
        let submitted = parse_component(json!({"type": 10, "id": 7}));
        let Component::TextDisplay(submitted) = submitted else {
            panic!("expected a text display");
        };
        assert_eq!(submitted.id, Some(7));
        assert_eq!(submitted.content, "");
    }

    #[test]
    fn parses_a_thumbnail_with_the_full_media_response() {
        let component = parse_component(json!({
            "type": 11,
            "id": 9,
            "media": {
                "url": "https://cdn.example.com/hero.webp",
                "proxy_url": "https://media.example.com/hero.webp",
                "height": 600,
                "width": 800,
                "placeholder": "1QcSHQRnh493V4dIh4eXh1h4kJUI",
                "placeholder_version": 1,
                "content_type": "image/webp",
                "flags": 1,
                "attachment_id": "111222333444555666"
            },
            "description": "A hero image",
            "spoiler": true
        }));

        let Component::Thumbnail(thumbnail) = component else {
            panic!("expected a thumbnail");
        };
        let media = thumbnail.media;
        assert_eq!(media.url, "https://cdn.example.com/hero.webp");
        assert_eq!(
            media.proxy_url.as_deref(),
            Some("https://media.example.com/hero.webp")
        );
        assert_eq!(media.height, Some(600));
        assert_eq!(media.width, Some(800));
        assert_eq!(media.content_type.as_deref(), Some("image/webp"));
        assert_eq!(media.placeholder_version, Some(1));
        assert_eq!(
            media.attachment_id,
            Some(AttachmentId::new(111_222_333_444_555_666))
        );
        assert!(media.is_animated());
        assert_eq!(thumbnail.description.as_deref(), Some("A hero image"));
        assert_eq!(thumbnail.spoiler, Some(true));
    }

    #[test]
    fn unfurled_media_items_only_report_animation_when_the_flag_is_set() {
        let still = UnfurledMediaItem::new("https://example.com/a.png");
        assert!(!still.is_animated());

        let animated = UnfurledMediaItem {
            flags: Some(UnfurledMediaItem::FLAG_IS_ANIMATED),
            ..UnfurledMediaItem::new("https://example.com/a.gif")
        };
        assert!(animated.is_animated());

        let other_flags = UnfurledMediaItem {
            flags: Some(2),
            ..UnfurledMediaItem::new("https://example.com/a.png")
        };
        assert!(!other_flags.is_animated());
        assert_eq!(UnfurledMediaItem::FLAG_IS_ANIMATED, 1);
    }

    #[test]
    fn parses_a_media_gallery() {
        let component = parse_component(json!({
            "type": 12,
            "id": 12,
            "items": [
                {
                    "media": {"url": "https://livevideofeedconvertedtoimage/webcam1.webp"},
                    "description": "An aerial view looking down on older industrial complex buildings."
                },
                {
                    "media": {"url": "https://livevideofeedconvertedtoimage/webcam2.webp"},
                    "spoiler": true
                }
            ]
        }));

        let Component::MediaGallery(gallery) = component else {
            panic!("expected a media gallery");
        };
        assert_eq!(gallery.items.len(), 2);
        assert_eq!(
            gallery.items[0].media.url,
            "https://livevideofeedconvertedtoimage/webcam1.webp"
        );
        assert!(gallery.items[0].description.is_some());
        assert_eq!(gallery.items[1].spoiler, Some(true));
        assert_eq!(gallery.items[1].description, None);
    }

    #[test]
    fn parses_a_file_component() {
        let component = parse_component(json!({
            "type": 13,
            "id": 13,
            "file": {"url": "attachment://game.zip"},
            "spoiler": false,
            "name": "game.zip",
            "size": 241_394
        }));

        let Component::File(file) = component else {
            panic!("expected a file");
        };
        assert_eq!(file.file.url, "attachment://game.zip");
        assert_eq!(file.spoiler, Some(false));
        assert_eq!(file.name.as_deref(), Some("game.zip"));
        assert_eq!(file.size, Some(241_394));
    }

    #[test]
    fn parses_a_separator_and_its_spacing_values() {
        let separator =
            parse_component(json!({"type": 14, "id": 14, "divider": false, "spacing": 2}));
        let Component::Separator(separator) = separator else {
            panic!("expected a separator");
        };
        assert_eq!(separator.divider, Some(false));
        assert_eq!(separator.spacing, Some(SeparatorSpacing::Large));
        assert_eq!(SeparatorSpacing::Large.as_i64(), 2);

        let default_spacing = parse_component(json!({"type": 14}));
        let Component::Separator(default_spacing) = default_spacing else {
            panic!("expected a separator");
        };
        assert_eq!(default_spacing.divider, None);
        assert_eq!(default_spacing.spacing, None);

        let unknown = parse_component(json!({"type": 14, "spacing": 3}));
        let Component::Separator(unknown) = unknown else {
            panic!("expected a separator");
        };
        assert_eq!(unknown.spacing, Some(SeparatorSpacing::Other(3)));
    }

    #[test]
    fn parses_a_container() {
        // The documented Container example (`docs/components-reference.md`,
        // Container).
        let component = parse_component(json!({
            "type": 17,
            "accent_color": 703_487,
            "components": [
                {"type": 10, "content": "# You have encountered a wild coyote!"},
                {
                    "type": 12,
                    "items": [
                        {"media": {"url": "https://websitewithopensourceimages/coyote.webp"}}
                    ]
                },
                {"type": 10, "content": "What would you like to do?"},
                {
                    "type": 1,
                    "components": [
                        {"type": 2, "custom_id": "pet_coyote", "label": "Pet it!", "style": 1},
                        {"type": 2, "custom_id": "feed_coyote", "label": "Attempt to feed it", "style": 2},
                        {"type": 2, "custom_id": "run_away", "label": "Run away!", "style": 4}
                    ]
                }
            ]
        }));

        let Component::Container(container) = component else {
            panic!("expected a container");
        };
        assert_eq!(container.accent_color, Some(703_487));
        assert_eq!(container.spoiler, None);
        assert_eq!(container.components.len(), 4);
        assert_eq!(container.components[0].kind(), ComponentKind::TextDisplay);
        assert_eq!(container.components[1].kind(), ComponentKind::MediaGallery);
        let row = container.components[3].as_action_row().expect("action row");
        assert_eq!(row.components.len(), 3);
        assert_eq!(row.components[2].custom_id(), Some("run_away"));
    }

    #[test]
    fn parses_a_file_upload_and_its_interaction_response() {
        let component = parse_component(json!({
            "type": 19,
            "id": 19,
            "custom_id": "file_upload",
            "min_values": 1,
            "max_values": 10,
            "required": true,
            "file_types": ["image", ".png"]
        }));

        let Component::FileUpload(upload) = component else {
            panic!("expected a file upload");
        };
        assert_eq!(upload.custom_id, "file_upload");
        assert_eq!(upload.min_values, Some(1));
        assert_eq!(upload.max_values, Some(10));
        assert_eq!(upload.required, Some(true));
        assert_eq!(
            upload.file_types,
            vec!["image".to_owned(), ".png".to_owned()]
        );
        assert!(upload.values.is_empty());

        // The documented File Upload interaction response carries `values`
        // (the uploaded attachment ids) and none of the create-only fields.
        let submitted = parse_component(json!({
            "type": 19,
            "id": 2,
            "custom_id": "file_upload",
            "values": ["111111111111111111111"]
        }));
        let Component::FileUpload(submitted) = submitted else {
            panic!("expected a file upload");
        };
        assert_eq!(submitted.values, vec!["111111111111111111111".to_owned()]);
        assert!(submitted.file_types.is_empty());
    }

    #[test]
    fn parses_a_radio_group_and_its_interaction_response() {
        let component = parse_component(json!({
            "type": 21,
            "id": 21,
            "custom_id": "class_radio",
            "required": true,
            "options": [
                {"value": "warrior", "label": "Warrior", "description": "Strong and brave"},
                {"value": "rogue", "label": "Rogue", "default": true}
            ]
        }));

        let Component::RadioGroup(group) = component else {
            panic!("expected a radio group");
        };
        assert_eq!(group.custom_id, "class_radio");
        assert_eq!(group.options.len(), 2);
        assert_eq!(group.options[0].value, "warrior");
        assert_eq!(
            group.options[0].description.as_deref(),
            Some("Strong and brave")
        );
        assert_eq!(group.options[1].default, Some(true));
        assert_eq!(group.value, None);

        let submitted = parse_component(json!({
            "type": 21,
            "id": 2,
            "custom_id": "class_radio",
            "value": "warrior"
        }));
        let Component::RadioGroup(submitted) = submitted else {
            panic!("expected a radio group");
        };
        assert_eq!(submitted.value.as_deref(), Some("warrior"));
        assert!(submitted.options.is_empty());
    }

    #[test]
    fn a_null_radio_group_value_is_none() {
        let submitted = parse_component(json!({
            "type": 21,
            "id": 2,
            "custom_id": "class_radio",
            "value": null
        }));

        let Component::RadioGroup(submitted) = submitted else {
            panic!("expected a radio group");
        };
        assert_eq!(submitted.value, None);
    }

    #[test]
    fn parses_a_checkbox_group_and_its_interaction_response() {
        let component = parse_component(json!({
            "type": 22,
            "id": 22,
            "custom_id": "event_checkbox",
            "min_values": 1,
            "max_values": 3,
            "required": true,
            "options": [
                {"value": "march-4", "label": "March 4th"},
                {"value": "march-5", "label": "March 5th", "description": "A Saturday"}
            ]
        }));

        let Component::CheckboxGroup(group) = component else {
            panic!("expected a checkbox group");
        };
        assert_eq!(group.custom_id, "event_checkbox");
        assert_eq!(group.options.len(), 2);
        assert_eq!(group.min_values, Some(1));
        assert_eq!(group.max_values, Some(3));

        let submitted = parse_component(json!({
            "type": 22,
            "id": 2,
            "custom_id": "event_checkbox",
            "values": ["march-5", "march-10", "march-4"]
        }));
        let Component::CheckboxGroup(submitted) = submitted else {
            panic!("expected a checkbox group");
        };
        assert_eq!(
            submitted.values,
            vec![
                "march-5".to_owned(),
                "march-10".to_owned(),
                "march-4".to_owned()
            ]
        );
    }

    #[test]
    fn parses_a_checkbox_and_its_interaction_response() {
        let component = parse_component(json!({
            "type": 23,
            "id": 23,
            "custom_id": "like_checkbox",
            "default": true
        }));

        let Component::Checkbox(checkbox) = component else {
            panic!("expected a checkbox");
        };
        assert_eq!(checkbox.custom_id, "like_checkbox");
        assert_eq!(checkbox.default, Some(true));
        assert_eq!(checkbox.value, None);

        let submitted = parse_component(json!({
            "type": 23,
            "id": 2,
            "custom_id": "like_checkbox",
            "value": false
        }));
        let Component::Checkbox(submitted) = submitted else {
            panic!("expected a checkbox");
        };
        assert_eq!(submitted.value, Some(false));
    }

    #[test]
    fn parses_a_modal_submit_with_label_wrapped_v2_inputs() {
        let label = parse_component(json!({
            "type": 18,
            "id": 1,
            "component": {
                "type": 19,
                "id": 2,
                "custom_id": "file_upload",
                "values": ["111111111111111111111"]
            }
        }));

        let Component::Label(label) = label else {
            panic!("expected a label");
        };
        assert_eq!(label.label, "");
        assert_eq!(label.component.kind(), ComponentKind::FileUpload);
        assert_eq!(label.component.custom_id(), Some("file_upload"));

        let radio = parse_component(json!({
            "type": 18,
            "id": 3,
            "component": {
                "type": 21,
                "id": 4,
                "custom_id": "class_radio",
                "value": "warrior"
            }
        }));
        let Component::Label(radio) = radio else {
            panic!("expected a label");
        };
        assert_eq!(
            radio
                .component
                .as_radio_group()
                .and_then(|group| group.value.as_deref()),
            Some("warrior")
        );

        let checkbox = parse_component(json!({
            "type": 18,
            "id": 5,
            "component": {
                "type": 23,
                "id": 6,
                "custom_id": "like_checkbox",
                "value": true
            }
        }));
        let Component::Label(checkbox) = checkbox else {
            panic!("expected a label");
        };
        assert_eq!(
            checkbox
                .component
                .as_checkbox()
                .and_then(|checkbox| checkbox.value),
            Some(true)
        );
    }

    #[test]
    fn unknown_component_types_are_preserved_raw() {
        for kind in [15, 16, 20, 24, 99] {
            let component = parse_component(json!({
                "type": kind,
                "id": 1,
                "some_future_field": {"nested": true}
            }));

            let Component::Other(unknown) = component else {
                panic!("component type {kind} should be preserved raw");
            };
            assert_eq!(unknown.kind, kind);
            assert_eq!(unknown.raw["type"], json!(kind));
            assert_eq!(unknown.raw["some_future_field"]["nested"], json!(true));
        }
    }

    #[test]
    fn unknown_fields_on_a_typed_component_are_ignored() {
        let display = parse_component(json!({
            "type": 10,
            "content": "Hello",
            "future_field": {"nested": true}
        }));

        assert_eq!(display.kind(), ComponentKind::TextDisplay);
        let Component::TextDisplay(display) = display else {
            panic!("expected a text display");
        };
        assert_eq!(display.content, "Hello");
    }

    #[test]
    fn a_malformed_known_component_degrades_to_other_instead_of_failing() {
        // A button without its required style is not a documented shape; it
        // must not fail the enclosing message.
        let component = parse_component(json!({"type": 2, "custom_id": "no-style"}));

        let Component::Other(unknown) = component else {
            panic!("expected the raw fallback");
        };
        assert_eq!(unknown.kind, 2);
        assert_eq!(unknown.raw["custom_id"], json!("no-style"));

        // A text display whose `content` is not a string is not a documented
        // shape either.
        let component = parse_component(json!({"type": 10, "content": 5}));
        assert!(matches!(component, Component::Other(unknown) if unknown.kind == 10));
    }

    #[test]
    fn a_component_without_a_type_is_preserved_raw() {
        let component = parse_component(json!({"custom_id": "missing-type"}));

        let Component::Other(unknown) = component else {
            panic!("expected the raw fallback");
        };
        assert_eq!(unknown.kind, 0);
    }

    #[test]
    fn component_kinds_round_trip_the_documented_integers() {
        for (value, kind) in [
            (1, ComponentKind::ActionRow),
            (2, ComponentKind::Button),
            (3, ComponentKind::StringSelect),
            (4, ComponentKind::TextInput),
            (5, ComponentKind::UserSelect),
            (6, ComponentKind::RoleSelect),
            (7, ComponentKind::MentionableSelect),
            (8, ComponentKind::ChannelSelect),
            (9, ComponentKind::Section),
            (10, ComponentKind::TextDisplay),
            (11, ComponentKind::Thumbnail),
            (12, ComponentKind::MediaGallery),
            (13, ComponentKind::File),
            (14, ComponentKind::Separator),
            (17, ComponentKind::Container),
            (18, ComponentKind::Label),
            (19, ComponentKind::FileUpload),
            (21, ComponentKind::RadioGroup),
            (22, ComponentKind::CheckboxGroup),
            (23, ComponentKind::Checkbox),
        ] {
            assert_eq!(ComponentKind::from_i64(value), kind);
            assert_eq!(kind.as_i64(), value);

            let serialised = serde_json::to_value(kind).expect("serialisable");
            assert_eq!(serialised, json!(value));
            let parsed: ComponentKind = serde_json::from_value(serialised).expect("deserialisable");
            assert_eq!(parsed, kind);
        }

        assert_eq!(ComponentKind::from_i64(15), ComponentKind::Other(15));
        assert_eq!(ComponentKind::Other(15).as_i64(), 15);
    }

    #[test]
    fn select_default_value_kinds_round_trip_the_documented_strings() {
        for value in ["user", "role", "channel"] {
            let kind = SelectDefaultValueKind::from_wire(value);
            assert_eq!(kind.as_str(), value);

            let serialised = serde_json::to_value(&kind).expect("serialisable");
            assert_eq!(serialised, json!(value));
        }

        assert_eq!(
            SelectDefaultValueKind::from_wire("something-new"),
            SelectDefaultValueKind::Other("something-new".to_owned())
        );
        assert_eq!(SelectDefaultValueKind::Other("x".to_owned()).as_str(), "x");
    }

    #[test]
    fn components_serialise_back_to_the_wire_shape() {
        let component = parse_component(json!({
            "type": 1,
            "components": [
                {"type": 2, "style": 1, "label": "OK", "custom_id": "ok"}
            ]
        }));

        let serialised = serde_json::to_value(&component).expect("serialisable");

        assert_eq!(serialised["type"], json!(1));
        assert_eq!(serialised["components"][0]["type"], json!(2));
        assert_eq!(serialised["components"][0]["style"], json!(1));
        assert_eq!(serialised["components"][0]["custom_id"], json!("ok"));

        let reparsed: Component = serde_json::from_value(serialised).expect("round trip");
        assert_eq!(reparsed, component);
    }

    #[test]
    fn v2_components_round_trip_through_serde() {
        let payloads = [
            json!({
                "type": 9,
                "components": [{"type": 10, "content": "# Title"}],
                "accessory": {"type": 11, "media": {"url": "https://example.com/a.webp"}}
            }),
            json!({"type": 10, "id": 2, "content": "markdown **text**"}),
            json!({
                "type": 11,
                "media": {"url": "https://example.com/a.webp"},
                "description": "alt",
                "spoiler": false
            }),
            json!({
                "type": 12,
                "items": [
                    {"media": {"url": "https://example.com/a.webp"}},
                    {"media": {"url": "https://example.com/b.webp"}, "spoiler": true}
                ]
            }),
            json!({"type": 13, "file": {"url": "attachment://manual.pdf"}, "spoiler": true}),
            json!({"type": 14, "divider": false, "spacing": 2}),
            json!({
                "type": 17,
                "accent_color": 5_793_266,
                "spoiler": true,
                "components": [
                    {"type": 10, "content": "inside"},
                    {"type": 14}
                ]
            }),
            json!({
                "type": 19,
                "custom_id": "file_upload",
                "min_values": 1,
                "max_values": 10,
                "required": true,
                "file_types": ["image"]
            }),
            json!({
                "type": 21,
                "custom_id": "class_radio",
                "options": [{"value": "warrior", "label": "Warrior"}],
                "required": true
            }),
            json!({
                "type": 22,
                "custom_id": "event_checkbox",
                "options": [{"value": "march-4", "label": "March 4th"}],
                "min_values": 0,
                "max_values": 10
            }),
            json!({"type": 23, "custom_id": "like_checkbox", "default": true}),
            json!({
                "type": 18,
                "label": "Pick one",
                "component": {"type": 21, "custom_id": "pick", "options": []}
            }),
        ];

        for payload in payloads {
            let component: Component =
                serde_json::from_value(payload.clone()).expect("documented component parses");
            let serialised = serde_json::to_value(&component).expect("serialisable");
            assert_eq!(serialised, payload);

            let reparsed: Component = serde_json::from_value(serialised).expect("round trip");
            assert_eq!(reparsed, component);
        }
    }

    #[test]
    fn unknown_components_serialise_the_original_json() {
        let raw = json!({"type": 99, "components": [], "accent_color": 1});
        let component: Component = serde_json::from_value(raw.clone()).expect("parsed");

        assert_eq!(serde_json::to_value(&component).expect("serialisable"), raw);
    }

    #[test]
    fn documented_limits_are_not_enforced_while_parsing() {
        // Parsers must never reject a payload just because a documented limit
        // is exceeded: validation is the sender's job, and a message from
        // Discord (or a future API revision) must still parse.
        let long = parse_component(json!({"type": 10, "content": "x".repeat(5_000)}));
        assert_eq!(long.kind(), ComponentKind::TextDisplay);

        let items: Vec<Value> = (0..12)
            .map(|index| json!({"media": {"url": format!("https://example.com/{index}.webp")}}))
            .collect();
        let gallery = parse_component(json!({"type": 12, "items": items}));
        let Component::MediaGallery(gallery) = gallery else {
            panic!("expected a media gallery");
        };
        assert_eq!(gallery.items.len(), 12);
    }

    #[test]
    fn accessors_return_none_for_other_variants() {
        let component = parse_component(json!({"type": 99, "content": "Hello"}));

        assert_eq!(component.kind(), ComponentKind::Other(99));
        assert_eq!(component.custom_id(), None);
        assert!(component.as_action_row().is_none());
        assert!(component.as_label().is_none());
        assert!(component.as_section().is_none());
        assert!(component.as_container().is_none());
        assert!(component.as_text_display().is_none());
    }

    #[test]
    fn new_component_kinds_have_their_typed_accessors() {
        let display = parse_component(json!({"type": 10, "content": "hi"}));
        assert_eq!(
            display
                .as_text_display()
                .map(|display| display.content.as_str()),
            Some("hi")
        );
        assert!(display.as_section().is_none());

        let gallery = parse_component(json!({"type": 12, "items": []}));
        assert!(gallery.as_media_gallery().is_some());

        let file = parse_component(json!({"type": 13, "file": {"url": "attachment://a.txt"}}));
        assert!(file.as_file().is_some());

        let separator = parse_component(json!({"type": 14}));
        assert!(separator.as_separator().is_some());

        let container = parse_component(json!({"type": 17, "components": []}));
        assert!(container.as_container().is_some());

        let thumbnail =
            parse_component(json!({"type": 11, "media": {"url": "https://example.com/a.png"}}));
        assert!(thumbnail.as_thumbnail().is_some());

        let upload = parse_component(json!({"type": 19, "custom_id": "upload"}));
        assert!(upload.as_file_upload().is_some());

        let radio = parse_component(json!({"type": 21, "custom_id": "radio", "options": []}));
        assert!(radio.as_radio_group().is_some());

        let checkbox_group =
            parse_component(json!({"type": 22, "custom_id": "group", "options": []}));
        assert!(checkbox_group.as_checkbox_group().is_some());

        let checkbox = parse_component(json!({"type": 23, "custom_id": "checkbox"}));
        assert!(checkbox.as_checkbox().is_some());

        let string_select = parse_component(json!({"type": 3, "custom_id": "select"}));
        assert!(string_select.as_string_select().is_some());

        let text_input = parse_component(json!({"type": 4, "custom_id": "input"}));
        assert!(text_input.as_text_input().is_some());

        let button = parse_component(json!({"type": 2, "style": 1, "custom_id": "button"}));
        assert!(button.as_button().is_some());

        let user_select = parse_component(json!({"type": 5, "custom_id": "users"}));
        assert!(user_select.as_user_select().is_some());

        let role_select = parse_component(json!({"type": 6, "custom_id": "roles"}));
        assert!(role_select.as_role_select().is_some());

        let mentionable = parse_component(json!({"type": 7, "custom_id": "mentions"}));
        assert!(mentionable.as_mentionable_select().is_some());

        let channel = parse_component(json!({"type": 8, "custom_id": "channels"}));
        assert!(channel.as_channel_select().is_some());
    }

    #[test]
    fn custom_ids_are_exposed_for_the_interactive_v2_components() {
        for (payload, expected) in [
            (json!({"type": 19, "custom_id": "upload"}), Some("upload")),
            (
                json!({"type": 21, "custom_id": "radio", "options": []}),
                Some("radio"),
            ),
            (
                json!({"type": 22, "custom_id": "group", "options": []}),
                Some("group"),
            ),
            (
                json!({"type": 23, "custom_id": "checkbox"}),
                Some("checkbox"),
            ),
            (json!({"type": 10, "content": "no id"}), None),
            (json!({"type": 17, "components": []}), None),
        ] {
            assert_eq!(parse_component(payload).custom_id(), expected);
        }
    }
}
