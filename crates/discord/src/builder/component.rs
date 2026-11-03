//! Builders for every documented component object, including the
//! `IS_COMPONENTS_V2` layout and content components.
//!
//! A component object is documented in `docs/components-reference.md`
//! (Component Object, plus one section per component type). The interactive
//! message/modal builders live in [`crate::builder::interaction`]
//! ([`ActionRowBuilder`], [`ButtonBuilder`], the selects,
//! [`TextInputBuilder`] and [`LabelBuilder`]); this module adds the
//! `IS_COMPONENTS_V2` layout and content components:
//!
//! * [`SectionBuilder`] (`9`), with a [`SectionAccessoryBuilder`] accessory;
//! * [`TextDisplayBuilder`] (`10`);
//! * [`ThumbnailBuilder`] (`11`) and [`MediaGalleryBuilder`] (`12`);
//! * [`FileBuilder`] (`13`);
//! * [`SeparatorBuilder`] (`14`);
//! * [`ContainerBuilder`] (`17`);
//! * the modal inputs [`FileUploadBuilder`] (`19`),
//!   [`RadioGroupBuilder`] (`21`), [`CheckboxGroupBuilder`] (`22`) and
//!   [`CheckboxBuilder`] (`23`).
//!
//! [`MessageComponentBuilder`] unions every outbound component builder so a
//! message, reply, update, edit or modal builder can accept one
//! `impl Into<MessageComponentBuilder>` argument. [`serialise_components`]
//! serialises a slice of them and clips to the documented 40-component message
//! limit.
//!
//! Sending any of the layout/content components in a message requires the
//! `IS_COMPONENTS_V2` message flag, [`IS_COMPONENTS_V2_FLAG`] (`1 << 15`,
//! `docs/message.md`, Message Flags; `docs/components-reference.md`, Component
//! Object); [`MessageComponentBuilder::requires_v2`] reports whether a
//! component needs it.
//!
//! Like the rest of the builder layer the serialisation methods are
//! infallible, so documented limits are enforced by clipping instead of
//! returning an error: a `custom_id` longer than 100 characters, a media
//! gallery with more than 10 items, a section with more than 3 children, a
//! separator spacing outside 1-2, an accent color outside 24 bits, and so on
//! are clipped to the documented range. Limits that cannot be met by clipping
//! (a radio group needs at least 2 options, a media gallery at least 1) are the
//! caller's responsibility and are documented on the builder.
//!
//! Reference: `docs/components-reference.md` (one section per component;
//! Component Object — "Messages allow up to 40 total components"),
//! `docs/component-embeds.md` (Media — the 2,048-character unfurled media item
//! url cap, the only documented maximum for the shared media structure),
//! `docs/message.md` (Message Flags).

use serde_json::{Map, Value};

use super::interaction::{
    ActionRowBuilder, ButtonBuilder, ChannelSelectBuilder, LabelBuilder, MentionableSelectBuilder,
    RoleSelectBuilder, StringSelectBuilder, TextInputBuilder, UserSelectBuilder,
};
use super::limits;
use crate::model::component::{ComponentKind, SeparatorSpacing};

/// The `IS_COMPONENTS_V2` message flag: `1 << 15`.
///
/// A message using the layout/content components must set this flag on create;
/// once set it cannot be removed from the message.
///
/// Reference: `docs/message.md` (Message Flags, `IS_COMPONENTS_V2`) and
/// `docs/components-reference.md` (Component Object).
pub const IS_COMPONENTS_V2_FLAG: u64 = 1 << 15;

/// The maximum number of components in one message: 40.
///
/// Reference: `docs/components-reference.md` (Component Object — "Messages
/// allow up to 40 total components").
pub const MAX_MESSAGE_COMPONENTS: usize = 40;

/// Maximum characters in an unfurled media item url.
///
/// Reference: `docs/component-embeds.md` (Media — "a max of 2,048 characters in
/// length").
const MEDIA_URL: usize = 2048;

/// Maximum characters in a thumbnail or media gallery item description.
///
/// Reference: `docs/components-reference.md` (Thumbnail Structure, Media
/// Gallery Item Structure — "max 1024 characters").
const MEDIA_DESCRIPTION: usize = 1024;

/// Maximum items in a media gallery.
///
/// Reference: `docs/components-reference.md` (Media Gallery Structure — "1 to
/// 10 media gallery items").
const MEDIA_GALLERY_ITEMS: usize = 10;

/// Maximum child components in a section.
///
/// Reference: `docs/components-reference.md` (Section Structure — "One to three
/// child components").
const SECTION_COMPONENTS: usize = 3;

/// Maximum characters in a text display's `content`.
///
/// The Component Reference does not repeat a number in the Text
/// Display structure table; a text display is "extremely similar" to a
/// message's `content` (`docs/components-reference.md`, Text Display), and the
/// API's content-component cap matches the 4000-character component value cap
/// the reference gives for a text input (`docs/components-reference.md`, Text
/// Input Structure). The builder clips to 4000 rather than inventing a
/// stricter limit.
const TEXT_DISPLAY_CONTENT: usize = 4000;

/// Maximum files in one file upload.
///
/// Reference: `docs/components-reference.md` (File Upload Structure —
/// `min_values`/`max_values` 0-10).
const FILE_UPLOAD_FILES: u32 = 10;

/// Maximum file type filters in one file upload.
///
/// Reference: `docs/components-reference.md` (File Upload Structure —
/// `file_types` max 10).
const FILE_UPLOAD_TYPES: usize = 10;

/// Maximum options in a radio group.
///
/// Reference: `docs/components-reference.md` (Radio Group Structure — "min 2,
/// max 10").
const RADIO_GROUP_OPTIONS: usize = 10;

/// Maximum options in a checkbox group.
///
/// Reference: `docs/components-reference.md` (Checkbox Group Structure — "min
/// 1, max 10").
const CHECKBOX_GROUP_OPTIONS: usize = 10;

/// Maximum `min_values`/`max_values` of a checkbox group.
///
/// Reference: `docs/components-reference.md` (Checkbox Group Structure — "min
/// 0, max 10" and "min 1, max 10").
const CHECKBOX_GROUP_VALUES: u32 = 10;

/// Maximum accent color of a container, `0xFFFFFF`.
///
/// Reference: `docs/components-reference.md` (Container Structure — "RGB from
/// `0x000000` to `0xFFFFFF`").
const ACCENT_COLOR_MAX: u32 = 0x00FF_FFFF;

/// Creates a component object with its documented `type` integer.
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

/// Builds an unfurled media item (component type's `media`/`file` field).
///
/// Only `url` is settable when sending; Discord fills in the proxy url, size,
/// content type, placeholder and flags in the response
/// (`docs/components-reference.md`, Unfurled Media Item).
#[derive(Clone, Debug)]
pub struct UnfurledMediaItemBuilder {
    url: String,
}

impl UnfurledMediaItemBuilder {
    /// Creates a media item from a url or an `attachment://<filename>`
    /// reference (maximum 2048 characters).
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    /// Serialises the media item: `{"url": "..."}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "url".to_owned(),
            Value::String(limits::truncate(&self.url, MEDIA_URL)),
        );

        Value::Object(object)
    }
}

impl From<&str> for UnfurledMediaItemBuilder {
    fn from(url: &str) -> Self {
        Self::new(url)
    }
}

impl From<String> for UnfurledMediaItemBuilder {
    fn from(url: String) -> Self {
        Self::new(url)
    }
}

impl From<UnfurledMediaItemBuilder> for Value {
    fn from(builder: UnfurledMediaItemBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a text display (component type 10).
///
/// A text display is a top-level content component holding markdown text,
/// including mentions and emojis (`docs/components-reference.md`, Text
/// Display). The content is clipped to 4000 characters; `TEXT_DISPLAY_CONTENT`
/// documents where that number comes from.
#[derive(Clone, Debug)]
pub struct TextDisplayBuilder {
    content: String,
}

impl TextDisplayBuilder {
    /// Creates a text display with the given markdown content.
    #[must_use]
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }

    /// Replaces the markdown content (clipped to 4000 characters).
    #[must_use]
    pub fn content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    /// Serialises the text display: `{"type": 10, "content": "..."}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        component_object(
            ComponentKind::TextDisplay,
            [(
                "content".to_owned(),
                Value::String(limits::truncate(&self.content, TEXT_DISPLAY_CONTENT)),
            )],
        )
    }
}

impl From<TextDisplayBuilder> for Value {
    fn from(builder: TextDisplayBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a thumbnail (component type 11).
///
/// A thumbnail is a content component used as a section accessory
/// (`docs/components-reference.md`, Thumbnail). The description is clipped to
/// 1024 characters.
#[derive(Clone, Debug)]
pub struct ThumbnailBuilder {
    media: UnfurledMediaItemBuilder,
    description: Option<String>,
    spoiler: Option<bool>,
}

impl ThumbnailBuilder {
    /// Creates a thumbnail showing `media`.
    #[must_use]
    pub fn new(media: impl Into<UnfurledMediaItemBuilder>) -> Self {
        Self {
            media: media.into(),
            description: None,
            spoiler: None,
        }
    }

    /// Replaces the media shown by the thumbnail.
    #[must_use]
    pub fn media(mut self, media: impl Into<UnfurledMediaItemBuilder>) -> Self {
        self.media = media.into();
        self
    }

    /// Sets the alt text (maximum 1024 characters).
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets whether the thumbnail is a spoiler.
    #[must_use]
    pub const fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }

    /// Serialises the thumbnail:
    /// `{"type": 11, "media": {...}, "description"?, "spoiler"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("media".to_owned(), self.media.to_json());
        insert_string(
            &mut object,
            "description",
            self.description.as_ref(),
            MEDIA_DESCRIPTION,
        );
        insert_bool(&mut object, "spoiler", self.spoiler);

        component_object(ComponentKind::Thumbnail, object)
    }
}

impl From<ThumbnailBuilder> for Value {
    fn from(builder: ThumbnailBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds one item of a media gallery.
///
/// Reference: `docs/components-reference.md` (Media Gallery, Media Gallery Item
/// Structure — description max 1024 characters).
#[derive(Clone, Debug)]
pub struct MediaGalleryItemBuilder {
    media: UnfurledMediaItemBuilder,
    description: Option<String>,
    spoiler: Option<bool>,
}

impl MediaGalleryItemBuilder {
    /// Creates a gallery item showing `media`.
    #[must_use]
    pub fn new(media: impl Into<UnfurledMediaItemBuilder>) -> Self {
        Self {
            media: media.into(),
            description: None,
            spoiler: None,
        }
    }

    /// Sets the alt text (maximum 1024 characters).
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets whether the media is a spoiler.
    #[must_use]
    pub const fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }

    /// Serialises the gallery item: `{"media": {...}, "description"?,
    /// "spoiler"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("media".to_owned(), self.media.to_json());
        insert_string(
            &mut object,
            "description",
            self.description.as_ref(),
            MEDIA_DESCRIPTION,
        );
        insert_bool(&mut object, "spoiler", self.spoiler);

        Value::Object(object)
    }
}

impl From<MediaGalleryItemBuilder> for Value {
    fn from(builder: MediaGalleryItemBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a media gallery (component type 12).
///
/// A media gallery shows 1-10 image/video items
/// (`docs/components-reference.md`, Media Gallery). The builder clips the item
/// list to 10; it cannot invent the documented minimum of one item when the
/// caller sets none.
#[derive(Clone, Debug, Default)]
pub struct MediaGalleryBuilder {
    items: Vec<MediaGalleryItemBuilder>,
}

impl MediaGalleryBuilder {
    /// Creates an empty media gallery.
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Appends one gallery item; items after the tenth are ignored.
    #[must_use]
    pub fn item(mut self, item: MediaGalleryItemBuilder) -> Self {
        self.items.push(item);
        self
    }

    /// Appends gallery items; items after the tenth are ignored.
    #[must_use]
    pub fn items(mut self, items: impl IntoIterator<Item = MediaGalleryItemBuilder>) -> Self {
        self.items.extend(items);
        self
    }

    /// Serialises the media gallery: `{"type": 12, "items": [...]}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let items = self
            .items
            .iter()
            .take(MEDIA_GALLERY_ITEMS)
            .map(MediaGalleryItemBuilder::to_json)
            .collect();

        component_object(
            ComponentKind::MediaGallery,
            [("items".to_owned(), Value::Array(items))],
        )
    }
}

impl From<MediaGalleryBuilder> for Value {
    fn from(builder: MediaGalleryBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a file (component type 13).
///
/// A file displays an uploaded attachment referenced with the
/// `attachment://<filename>` protocol (`docs/components-reference.md`, File).
/// The `name` and `size` fields are provided by the API in the response and are
/// not settable when sending.
#[derive(Clone, Debug)]
pub struct FileBuilder {
    file: UnfurledMediaItemBuilder,
    spoiler: Option<bool>,
}

impl FileBuilder {
    /// Creates a file component for `file` (an `attachment://` media item).
    #[must_use]
    pub fn new(file: impl Into<UnfurledMediaItemBuilder>) -> Self {
        Self {
            file: file.into(),
            spoiler: None,
        }
    }

    /// Replaces the attached file.
    #[must_use]
    pub fn file(mut self, file: impl Into<UnfurledMediaItemBuilder>) -> Self {
        self.file = file.into();
        self
    }

    /// Sets whether the file is a spoiler.
    #[must_use]
    pub const fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }

    /// Serialises the file component: `{"type": 13, "file": {...},
    /// "spoiler"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("file".to_owned(), self.file.to_json());
        insert_bool(&mut object, "spoiler", self.spoiler);

        component_object(ComponentKind::File, object)
    }
}

impl From<FileBuilder> for Value {
    fn from(builder: FileBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a separator (component type 14).
///
/// A separator adds vertical padding and visual division between components
/// (`docs/components-reference.md`, Separator). The serializer only emits the
/// fields that are set, so the documented defaults (`divider` `true`, `spacing`
/// `1`) are left to the API when the caller does not override them.
#[derive(Clone, Debug, Default)]
pub struct SeparatorBuilder {
    divider: Option<bool>,
    spacing: Option<SeparatorSpacing>,
}

impl SeparatorBuilder {
    /// Creates a separator with the documented defaults.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            divider: None,
            spacing: None,
        }
    }

    /// Sets whether a visual divider is displayed (defaults to `true`).
    #[must_use]
    pub const fn divider(mut self, divider: bool) -> Self {
        self.divider = Some(divider);
        self
    }

    /// Sets the padding size (defaults to [`SeparatorSpacing::Small`]).
    #[must_use]
    pub const fn spacing(mut self, spacing: SeparatorSpacing) -> Self {
        self.spacing = Some(spacing);
        self
    }

    /// Serialises the separator: `{"type": 14, "divider"?, "spacing"?}`.
    ///
    /// A spacing value outside the documented 1-2 range (for example
    /// [`SeparatorSpacing::Other`]) is clipped to it.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        insert_bool(&mut object, "divider", self.divider);
        if let Some(spacing) = self.spacing {
            object.insert(
                "spacing".to_owned(),
                Value::from(spacing.as_i64().clamp(1, 2)),
            );
        }

        component_object(ComponentKind::Separator, object)
    }
}

impl From<SeparatorBuilder> for Value {
    fn from(builder: SeparatorBuilder) -> Self {
        builder.to_json()
    }
}

/// The accessory of a section: a button or a thumbnail.
///
/// Reference: `docs/components-reference.md` (Section, Section Accessory
/// Components — Button or Thumbnail).
#[derive(Clone, Debug)]
pub enum SectionAccessoryBuilder {
    /// A button accessory (component type 2).
    Button(ButtonBuilder),
    /// A thumbnail accessory (component type 11).
    Thumbnail(ThumbnailBuilder),
}

impl SectionAccessoryBuilder {
    /// Serialises the accessory component object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Button(button) => button.to_json(),
            Self::Thumbnail(thumbnail) => thumbnail.to_json(),
        }
    }
}

impl From<ButtonBuilder> for SectionAccessoryBuilder {
    fn from(button: ButtonBuilder) -> Self {
        Self::Button(button)
    }
}

impl From<ThumbnailBuilder> for SectionAccessoryBuilder {
    fn from(thumbnail: ThumbnailBuilder) -> Self {
        Self::Thumbnail(thumbnail)
    }
}

impl From<SectionAccessoryBuilder> for Value {
    fn from(builder: SectionAccessoryBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a section (component type 9).
///
/// A section associates one to three content components with an accessory
/// component (`docs/components-reference.md`, Section). The builder clips the
/// content to three children; the accessory should be a [`ButtonBuilder`] or a
/// [`ThumbnailBuilder`] (`SectionAccessoryBuilder`).
#[derive(Clone, Debug, Default)]
pub struct SectionBuilder {
    components: Vec<MessageComponentBuilder>,
    accessory: Option<SectionAccessoryBuilder>,
}

impl SectionBuilder {
    /// Creates an empty section.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            components: Vec::new(),
            accessory: None,
        }
    }

    /// Appends one content component; children after the third are ignored.
    #[must_use]
    pub fn component(mut self, component: impl Into<MessageComponentBuilder>) -> Self {
        if self.components.len() < SECTION_COMPONENTS {
            self.components.push(component.into());
        }
        self
    }

    /// Appends content components; children after the third are ignored.
    #[must_use]
    pub fn components<I, C>(mut self, components: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<MessageComponentBuilder>,
    {
        let room = SECTION_COMPONENTS.saturating_sub(self.components.len());
        self.components
            .extend(components.into_iter().take(room).map(Into::into));
        self
    }

    /// Appends a text display, the documented section content component.
    #[must_use]
    pub fn text_display(self, display: TextDisplayBuilder) -> Self {
        self.component(display)
    }

    /// Sets the accessory contextually associated with the content.
    #[must_use]
    pub fn accessory(mut self, accessory: impl Into<SectionAccessoryBuilder>) -> Self {
        self.accessory = Some(accessory.into());
        self
    }

    /// Serialises the section: `{"type": 9, "components": [...],
    /// "accessory"}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "components".to_owned(),
            Value::Array(
                self.components
                    .iter()
                    .map(MessageComponentBuilder::to_json)
                    .collect(),
            ),
        );
        if let Some(accessory) = &self.accessory {
            object.insert("accessory".to_owned(), accessory.to_json());
        }

        component_object(ComponentKind::Section, object)
    }
}

impl From<SectionBuilder> for Value {
    fn from(builder: SectionBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a container (component type 17).
///
/// A container visually groups components and has an optional accent color bar
/// (`docs/components-reference.md`, Container). The accent color is clipped to
/// 24 bits. The Component Reference lists the valid children (action rows,
/// text displays, sections, media galleries, separators and files); the builder
/// accepts any [`MessageComponentBuilder`] and leaves the child kinds to the
/// caller.
#[derive(Clone, Debug, Default)]
pub struct ContainerBuilder {
    components: Vec<MessageComponentBuilder>,
    accent_color: Option<u32>,
    spoiler: Option<bool>,
}

impl ContainerBuilder {
    /// Creates an empty container.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            components: Vec::new(),
            accent_color: None,
            spoiler: None,
        }
    }

    /// Appends one child component.
    #[must_use]
    pub fn component(mut self, component: impl Into<MessageComponentBuilder>) -> Self {
        self.components.push(component.into());
        self
    }

    /// Appends child components.
    #[must_use]
    pub fn components<I, C>(mut self, components: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<MessageComponentBuilder>,
    {
        self.components
            .extend(components.into_iter().map(Into::into));
        self
    }

    /// Appends an action row, a documented container child.
    #[must_use]
    pub fn action_row(self, row: ActionRowBuilder) -> Self {
        self.component(row)
    }

    /// Appends a text display, a documented container child.
    #[must_use]
    pub fn text_display(self, display: TextDisplayBuilder) -> Self {
        self.component(display)
    }

    /// Appends a section, a documented container child.
    #[must_use]
    pub fn section(self, section: SectionBuilder) -> Self {
        self.component(section)
    }

    /// Appends a media gallery, a documented container child.
    #[must_use]
    pub fn media_gallery(self, gallery: MediaGalleryBuilder) -> Self {
        self.component(gallery)
    }

    /// Appends a separator, a documented container child.
    #[must_use]
    pub fn separator(self, separator: SeparatorBuilder) -> Self {
        self.component(separator)
    }

    /// Appends a file, a documented container child.
    #[must_use]
    pub fn file(self, file: FileBuilder) -> Self {
        self.component(file)
    }

    /// Sets the accent color (24-bit RGB, clipped to `0xFFFFFF`).
    #[must_use]
    pub const fn accent_color(mut self, accent_color: u32) -> Self {
        self.accent_color = Some(accent_color & ACCENT_COLOR_MAX);
        self
    }

    /// Sets whether the container is a spoiler.
    #[must_use]
    pub const fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = Some(spoiler);
        self
    }

    /// Serialises the container: `{"type": 17, "components": [...],
    /// "accent_color"?, "spoiler"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "components".to_owned(),
            Value::Array(
                self.components
                    .iter()
                    .map(MessageComponentBuilder::to_json)
                    .collect(),
            ),
        );
        insert_number(&mut object, "accent_color", self.accent_color);
        insert_bool(&mut object, "spoiler", self.spoiler);

        component_object(ComponentKind::Container, object)
    }
}

impl From<ContainerBuilder> for Value {
    fn from(builder: ContainerBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a file upload (component type 19), a modal input for uploading
/// files.
///
/// `min_values`/`max_values` are clipped to 0-10, `file_types` to 10 entries,
/// and the documented rule that `min_values` is at least 1 unless `required` is
/// false is applied. File uploads must be placed inside a [`LabelBuilder`]
/// (`docs/components-reference.md`, File Upload).
#[derive(Clone, Debug)]
pub struct FileUploadBuilder {
    custom_id: String,
    min_values: Option<u32>,
    max_values: Option<u32>,
    required: Option<bool>,
    file_types: Vec<String>,
}

impl FileUploadBuilder {
    /// Creates a file upload with the given `custom_id` (1-100 characters).
    #[must_use]
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            min_values: None,
            max_values: None,
            required: None,
            file_types: Vec::new(),
        }
    }

    /// Sets the minimum number of files (0-10, clipped).
    #[must_use]
    pub const fn min_values(mut self, min_values: u32) -> Self {
        self.min_values = Some(min_values);
        self
    }

    /// Sets the maximum number of files (0-10, clipped).
    #[must_use]
    pub const fn max_values(mut self, max_values: u32) -> Self {
        self.max_values = Some(max_values);
        self
    }

    /// Sets whether files must be uploaded before submitting.
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Appends one accepted file type (`image`, `video`, `audio` or a
    /// dot-prefixed extension such as `.pdf`).
    #[must_use]
    pub fn file_type(mut self, file_type: impl Into<String>) -> Self {
        self.file_types.push(file_type.into());
        self
    }

    /// Appends accepted file types (the first 10 are serialised).
    #[must_use]
    pub fn file_types<S, I>(mut self, file_types: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.file_types
            .extend(file_types.into_iter().map(Into::into));
        self
    }

    /// Serialises the file upload: `{"type": 19, "custom_id", "min_values"?,
    /// "max_values"?, "required"?, "file_types"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );

        let required = self.required.unwrap_or(true);
        let min_values = self.min_values.map(|value| {
            let clipped = value.min(FILE_UPLOAD_FILES);
            if required { clipped.max(1) } else { clipped }
        });
        let mut max_values = self.max_values.map(|value| value.min(FILE_UPLOAD_FILES));
        if let (Some(max), Some(min)) = (max_values, min_values) {
            max_values = Some(max.max(min));
        }

        insert_number(&mut object, "min_values", min_values);
        insert_number(&mut object, "max_values", max_values);
        insert_bool(&mut object, "required", self.required);
        if !self.file_types.is_empty() {
            object.insert(
                "file_types".to_owned(),
                Value::Array(
                    self.file_types
                        .iter()
                        .take(FILE_UPLOAD_TYPES)
                        .map(|file_type| Value::String(file_type.clone()))
                        .collect(),
                ),
            );
        }

        component_object(ComponentKind::FileUpload, object)
    }
}

impl From<FileUploadBuilder> for Value {
    fn from(builder: FileUploadBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds one option of a [`RadioGroupBuilder`] or a
/// [`CheckboxGroupBuilder`].
///
/// Both documented option structures have the same fields: `value`, `label`,
/// optional `description` and optional `default`
/// (`docs/components-reference.md`, Radio Group Option Structure and Checkbox
/// Group Option Structure). The strings are clipped to 100 characters.
#[derive(Clone, Debug)]
pub struct GroupOptionBuilder {
    value: String,
    label: String,
    description: Option<String>,
    default: Option<bool>,
}

impl GroupOptionBuilder {
    /// Creates an option with the given value and label (1-100 characters each,
    /// clipped when serialised).
    #[must_use]
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            description: None,
            default: None,
        }
    }

    /// Sets the option description (maximum 100 characters, clipped).
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets whether the option is selected by default.
    #[must_use]
    pub const fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    /// Serialises the option: `{"value", "label", "description"?,
    /// "default"?}` with the strings clipped to 100 characters.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "value".to_owned(),
            Value::String(limits::truncate(&self.value, limits::SELECT_OPTION_TEXT)),
        );
        object.insert(
            "label".to_owned(),
            Value::String(limits::truncate(&self.label, limits::SELECT_OPTION_TEXT)),
        );
        insert_string(
            &mut object,
            "description",
            self.description.as_ref(),
            limits::SELECT_OPTION_TEXT,
        );
        insert_bool(&mut object, "default", self.default);

        Value::Object(object)
    }
}

impl From<GroupOptionBuilder> for Value {
    fn from(builder: GroupOptionBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a radio group (component type 21), a modal input for selecting
/// exactly one of two to ten options.
///
/// The builder clips the option list to ten; it cannot invent the documented
/// minimum of two options when the caller supplies fewer. Radio groups must be
/// placed inside a [`LabelBuilder`] (`docs/components-reference.md`, Radio
/// Group).
#[derive(Clone, Debug)]
pub struct RadioGroupBuilder {
    custom_id: String,
    options: Vec<GroupOptionBuilder>,
    required: Option<bool>,
}

impl RadioGroupBuilder {
    /// Creates a radio group with the given `custom_id` (1-100 characters).
    #[must_use]
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            options: Vec::new(),
            required: None,
        }
    }

    /// Appends one option; options after the tenth are ignored.
    #[must_use]
    pub fn option(mut self, option: GroupOptionBuilder) -> Self {
        self.options.push(option);
        self
    }

    /// Appends options; options after the tenth are ignored.
    #[must_use]
    pub fn options(mut self, options: impl IntoIterator<Item = GroupOptionBuilder>) -> Self {
        self.options.extend(options);
        self
    }

    /// Sets whether a selection is required to submit.
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Serialises the radio group: `{"type": 21, "custom_id", "options",
    /// "required"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );
        object.insert(
            "options".to_owned(),
            Value::Array(
                self.options
                    .iter()
                    .take(RADIO_GROUP_OPTIONS)
                    .map(GroupOptionBuilder::to_json)
                    .collect(),
            ),
        );
        insert_bool(&mut object, "required", self.required);

        component_object(ComponentKind::RadioGroup, object)
    }
}

impl From<RadioGroupBuilder> for Value {
    fn from(builder: RadioGroupBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a checkbox group (component type 22), a modal input for selecting one
/// or more of one to ten options.
///
/// The builder clips the option list to ten and `min_values`/`max_values` to
/// the documented 0-10/1-10 ranges, applies the documented rule that
/// `min_values` is at least 1 unless `required` is false, and keeps
/// `max_values` at least `min_values`. Checkbox groups must be placed inside a
/// [`LabelBuilder`] (`docs/components-reference.md`, Checkbox Group).
#[derive(Clone, Debug)]
pub struct CheckboxGroupBuilder {
    custom_id: String,
    options: Vec<GroupOptionBuilder>,
    min_values: Option<u32>,
    max_values: Option<u32>,
    required: Option<bool>,
}

impl CheckboxGroupBuilder {
    /// Creates a checkbox group with the given `custom_id` (1-100 characters).
    #[must_use]
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            options: Vec::new(),
            min_values: None,
            max_values: None,
            required: None,
        }
    }

    /// Appends one option; options after the tenth are ignored.
    #[must_use]
    pub fn option(mut self, option: GroupOptionBuilder) -> Self {
        self.options.push(option);
        self
    }

    /// Appends options; options after the tenth are ignored.
    #[must_use]
    pub fn options(mut self, options: impl IntoIterator<Item = GroupOptionBuilder>) -> Self {
        self.options.extend(options);
        self
    }

    /// Sets the minimum number of chosen options (0-10, clipped).
    #[must_use]
    pub const fn min_values(mut self, min_values: u32) -> Self {
        self.min_values = Some(min_values);
        self
    }

    /// Sets the maximum number of chosen options (1-10, clipped).
    #[must_use]
    pub const fn max_values(mut self, max_values: u32) -> Self {
        self.max_values = Some(max_values);
        self
    }

    /// Sets whether selecting within the group is required.
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Serialises the checkbox group: `{"type": 22, "custom_id", "options",
    /// "min_values"?, "max_values"?, "required"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );
        object.insert(
            "options".to_owned(),
            Value::Array(
                self.options
                    .iter()
                    .take(CHECKBOX_GROUP_OPTIONS)
                    .map(GroupOptionBuilder::to_json)
                    .collect(),
            ),
        );

        let required = self.required.unwrap_or(true);
        let min_values = self.min_values.map(|value| {
            let clipped = value.min(CHECKBOX_GROUP_VALUES);
            if required { clipped.max(1) } else { clipped }
        });
        let mut max_values = self
            .max_values
            .map(|value| value.clamp(1, CHECKBOX_GROUP_VALUES));
        if let (Some(max), Some(min)) = (max_values, min_values) {
            max_values = Some(max.max(min));
        }

        insert_number(&mut object, "min_values", min_values);
        insert_number(&mut object, "max_values", max_values);
        insert_bool(&mut object, "required", self.required);

        component_object(ComponentKind::CheckboxGroup, object)
    }
}

impl From<CheckboxGroupBuilder> for Value {
    fn from(builder: CheckboxGroupBuilder) -> Self {
        builder.to_json()
    }
}

/// Builds a checkbox (component type 23), a modal input for a single yes/no
/// question.
///
/// A checkbox must be placed inside a [`LabelBuilder`]
/// (`docs/components-reference.md`, Checkbox). The Component Reference does not
/// document a `required` field on a checkbox (it suggests a single-option
/// checkbox group instead), so [`CheckboxBuilder::required`] is only serialised
/// when the caller sets it explicitly.
#[derive(Clone, Debug)]
pub struct CheckboxBuilder {
    custom_id: String,
    default: Option<bool>,
    required: Option<bool>,
}

impl CheckboxBuilder {
    /// Creates a checkbox with the given `custom_id` (1-100 characters).
    #[must_use]
    pub fn new(custom_id: impl Into<String>) -> Self {
        Self {
            custom_id: custom_id.into(),
            default: None,
            required: None,
        }
    }

    /// Sets whether the checkbox is selected by default.
    #[must_use]
    pub const fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    /// Sets whether the checkbox is required.
    ///
    /// Not documented for a checkbox; prefer a single-option
    /// [`CheckboxGroupBuilder`] when a required yes/no choice is needed.
    #[must_use]
    pub const fn required(mut self, required: bool) -> Self {
        self.required = Some(required);
        self
    }

    /// Serialises the checkbox: `{"type": 23, "custom_id", "default"?,
    /// "required"?}`.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert(
            "custom_id".to_owned(),
            Value::String(limits::truncate(&self.custom_id, limits::CUSTOM_ID)),
        );
        insert_bool(&mut object, "default", self.default);
        insert_bool(&mut object, "required", self.required);

        component_object(ComponentKind::Checkbox, object)
    }
}

impl From<CheckboxBuilder> for Value {
    fn from(builder: CheckboxBuilder) -> Self {
        builder.to_json()
    }
}

/// Any outbound component builder.
///
/// This union lets a message, reply, update, edit or modal builder accept one
/// `impl Into<MessageComponentBuilder>` argument regardless of the component
/// type, and lets [`serialise_components`] serialise a mixed component list.
///
/// Reference: `docs/components-reference.md` (Component Types).
#[derive(Clone, Debug)]
pub enum MessageComponentBuilder {
    /// An action row (component type 1).
    ActionRow(ActionRowBuilder),
    /// A button (component type 2).
    Button(ButtonBuilder),
    /// A string select (component type 3).
    StringSelect(StringSelectBuilder),
    /// A text input (component type 4).
    TextInput(TextInputBuilder),
    /// A user select (component type 5).
    UserSelect(UserSelectBuilder),
    /// A role select (component type 6).
    RoleSelect(RoleSelectBuilder),
    /// A mentionable select (component type 7).
    MentionableSelect(MentionableSelectBuilder),
    /// A channel select (component type 8).
    ChannelSelect(ChannelSelectBuilder),
    /// A section (component type 9).
    Section(SectionBuilder),
    /// A text display (component type 10).
    TextDisplay(TextDisplayBuilder),
    /// A thumbnail (component type 11).
    Thumbnail(ThumbnailBuilder),
    /// A media gallery (component type 12).
    MediaGallery(MediaGalleryBuilder),
    /// A file (component type 13).
    File(FileBuilder),
    /// A separator (component type 14).
    Separator(SeparatorBuilder),
    /// A container (component type 17).
    Container(ContainerBuilder),
    /// A label (component type 18).
    Label(LabelBuilder),
    /// A file upload (component type 19).
    FileUpload(FileUploadBuilder),
    /// A radio group (component type 21).
    RadioGroup(RadioGroupBuilder),
    /// A checkbox group (component type 22).
    CheckboxGroup(CheckboxGroupBuilder),
    /// A checkbox (component type 23).
    Checkbox(CheckboxBuilder),
}

impl MessageComponentBuilder {
    /// Serialises the component object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::ActionRow(builder) => builder.to_json(),
            Self::Button(builder) => builder.to_json(),
            Self::StringSelect(builder) => builder.to_json(),
            Self::TextInput(builder) => builder.to_json(),
            Self::UserSelect(builder) => builder.to_json(),
            Self::RoleSelect(builder) => builder.to_json(),
            Self::MentionableSelect(builder) => builder.to_json(),
            Self::ChannelSelect(builder) => builder.to_json(),
            Self::Section(builder) => builder.to_json(),
            Self::TextDisplay(builder) => builder.to_json(),
            Self::Thumbnail(builder) => builder.to_json(),
            Self::MediaGallery(builder) => builder.to_json(),
            Self::File(builder) => builder.to_json(),
            Self::Separator(builder) => builder.to_json(),
            Self::Container(builder) => builder.to_json(),
            Self::Label(builder) => builder.to_json(),
            Self::FileUpload(builder) => builder.to_json(),
            Self::RadioGroup(builder) => builder.to_json(),
            Self::CheckboxGroup(builder) => builder.to_json(),
            Self::Checkbox(builder) => builder.to_json(),
        }
    }

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
        }
    }

    /// Returns whether sending this component in a message requires the
    /// [`IS_COMPONENTS_V2_FLAG`] message flag.
    ///
    /// The layout and content component types (`9`, `10`, `11`, `12`, `13`,
    /// `14` and `17`) are the `IS_COMPONENTS_V2` set
    /// (`docs/components-reference.md`, Component Types); the interactive
    /// components work in legacy messages (and the modal-only inputs never
    /// reach a message).
    #[must_use]
    pub fn requires_v2(&self) -> bool {
        matches!(
            self,
            Self::Section(_)
                | Self::TextDisplay(_)
                | Self::Thumbnail(_)
                | Self::MediaGallery(_)
                | Self::File(_)
                | Self::Separator(_)
                | Self::Container(_)
        )
    }
}

/// Implements `From<Builder> for MessageComponentBuilder` for the component
/// builders.
macro_rules! message_component_from {
    ($($variant:ident <- $builder:ty),+ $(,)?) => {
        $(
            impl From<$builder> for MessageComponentBuilder {
                fn from(builder: $builder) -> Self {
                    Self::$variant(builder)
                }
            }
        )+
    };
}

message_component_from!(
    ActionRow <- ActionRowBuilder,
    Button <- ButtonBuilder,
    StringSelect <- StringSelectBuilder,
    TextInput <- TextInputBuilder,
    UserSelect <- UserSelectBuilder,
    RoleSelect <- RoleSelectBuilder,
    MentionableSelect <- MentionableSelectBuilder,
    ChannelSelect <- ChannelSelectBuilder,
    Section <- SectionBuilder,
    TextDisplay <- TextDisplayBuilder,
    Thumbnail <- ThumbnailBuilder,
    MediaGallery <- MediaGalleryBuilder,
    File <- FileBuilder,
    Separator <- SeparatorBuilder,
    Container <- ContainerBuilder,
    Label <- LabelBuilder,
    FileUpload <- FileUploadBuilder,
    RadioGroup <- RadioGroupBuilder,
    CheckboxGroup <- CheckboxGroupBuilder,
    Checkbox <- CheckboxBuilder,
);

impl From<MessageComponentBuilder> for Value {
    fn from(builder: MessageComponentBuilder) -> Self {
        builder.to_json()
    }
}

/// Serialises a component list, clipped to the documented 40-component message
/// limit.
///
/// Reference: `docs/components-reference.md` (Component Object — "Messages
/// allow up to 40 total components"). The API counts nested components toward
/// that total too; this helper only caps the top-level array.
#[must_use]
pub fn serialise_components(components: &[MessageComponentBuilder]) -> Vec<Value> {
    components
        .iter()
        .take(MAX_MESSAGE_COMPONENTS)
        .map(MessageComponentBuilder::to_json)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::model::component::TextInputStyle;

    use super::*;

    #[test]
    fn text_display_serialises() {
        let json = TextDisplayBuilder::new("# Hello\nworld").to_json();

        assert_eq!(json, json!({"type": 10, "content": "# Hello\nworld"}));
    }

    #[test]
    fn text_display_content_is_clipped_to_four_thousand_characters() {
        let json = TextDisplayBuilder::new("x".repeat(5_000)).to_json();

        assert_eq!(
            json["content"]
                .as_str()
                .map(str::chars)
                .map(Iterator::count),
            Some(4_000)
        );
    }

    #[test]
    fn unfurled_media_items_serialise_the_url_only() {
        assert_eq!(
            UnfurledMediaItemBuilder::new("https://example.com/a.webp").to_json(),
            json!({"url": "https://example.com/a.webp"})
        );
        assert_eq!(
            UnfurledMediaItemBuilder::from("attachment://manual.pdf").to_json(),
            json!({"url": "attachment://manual.pdf"})
        );
    }

    #[test]
    fn unfurled_media_urls_are_clipped_to_the_documented_length() {
        let json =
            UnfurledMediaItemBuilder::new(format!("https://example.com/{}", "x".repeat(3_000)))
                .to_json();

        assert_eq!(
            json["url"].as_str().map(str::chars).map(Iterator::count),
            Some(2_048)
        );
    }

    #[test]
    fn thumbnail_serialises_media_and_optional_fields() {
        let json = ThumbnailBuilder::new("https://example.com/a.webp")
            .description("A hero image")
            .spoiler(true)
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 11,
                "media": {"url": "https://example.com/a.webp"},
                "description": "A hero image",
                "spoiler": true
            })
        );

        assert_eq!(
            ThumbnailBuilder::new("https://example.com/a.webp").to_json(),
            json!({"type": 11, "media": {"url": "https://example.com/a.webp"}})
        );
    }

    #[test]
    fn thumbnail_descriptions_are_clipped() {
        let json = ThumbnailBuilder::new("https://example.com/a.webp")
            .description("d".repeat(2_000))
            .to_json();

        assert_eq!(
            json["description"]
                .as_str()
                .map(str::chars)
                .map(Iterator::count),
            Some(1_024)
        );
    }

    #[test]
    fn media_gallery_serialises_items_with_descriptions_and_spoilers() {
        let json = MediaGalleryBuilder::new()
            .item(
                MediaGalleryItemBuilder::new("https://example.com/a.webp")
                    .description("The first image"),
            )
            .item(MediaGalleryItemBuilder::new("https://example.com/b.webp").spoiler(true))
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 12,
                "items": [
                    {
                        "media": {"url": "https://example.com/a.webp"},
                        "description": "The first image"
                    },
                    {
                        "media": {"url": "https://example.com/b.webp"},
                        "spoiler": true
                    }
                ]
            })
        );
    }

    #[test]
    fn media_galleries_are_clipped_to_ten_items() {
        let mut gallery = MediaGalleryBuilder::new();
        for index in 0..12 {
            gallery = gallery.item(MediaGalleryItemBuilder::new(format!(
                "https://example.com/{index}.webp"
            )));
        }

        let json = gallery.to_json();
        let items = json["items"].as_array().expect("items");
        assert_eq!(items.len(), 10);
        assert_eq!(
            items[9]["media"]["url"],
            json!("https://example.com/9.webp")
        );
    }

    #[test]
    fn file_serialises_the_attachment_reference() {
        assert_eq!(
            FileBuilder::new("attachment://game.zip").to_json(),
            json!({"type": 13, "file": {"url": "attachment://game.zip"}})
        );
        assert_eq!(
            FileBuilder::new("attachment://manual.pdf")
                .spoiler(true)
                .to_json(),
            json!({
                "type": 13,
                "file": {"url": "attachment://manual.pdf"},
                "spoiler": true
            })
        );
    }

    #[test]
    fn separator_only_serialises_overridden_fields() {
        assert_eq!(SeparatorBuilder::new().to_json(), json!({"type": 14}));
        assert_eq!(
            SeparatorBuilder::new()
                .divider(false)
                .spacing(SeparatorSpacing::Large)
                .to_json(),
            json!({"type": 14, "divider": false, "spacing": 2})
        );
    }

    #[test]
    fn separator_spacing_is_clipped_to_the_documented_range() {
        assert_eq!(
            SeparatorBuilder::new()
                .spacing(SeparatorSpacing::Other(0))
                .to_json()["spacing"],
            json!(1)
        );
        assert_eq!(
            SeparatorBuilder::new()
                .spacing(SeparatorSpacing::Other(3))
                .to_json()["spacing"],
            json!(2)
        );
    }

    #[test]
    fn section_serialises_content_and_a_thumbnail_accessory() {
        let json = SectionBuilder::new()
            .text_display(TextDisplayBuilder::new("# Real Game v7.3"))
            .text_display(TextDisplayBuilder::new("Hope you're excited!"))
            .accessory(ThumbnailBuilder::new(
                "https://websitewithopensourceimages/gamepreview.webp",
            ))
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 9,
                "components": [
                    {"type": 10, "content": "# Real Game v7.3"},
                    {"type": 10, "content": "Hope you're excited!"}
                ],
                "accessory": {
                    "type": 11,
                    "media": {"url": "https://websitewithopensourceimages/gamepreview.webp"}
                }
            })
        );
    }

    #[test]
    fn section_serialises_a_button_accessory() {
        let json = SectionBuilder::new()
            .text_display(TextDisplayBuilder::new("Open the page"))
            .accessory(ButtonBuilder::link(
                "https://watchanimeattheoffice.com/",
                "Open",
            ))
            .to_json();

        assert_eq!(
            json["accessory"],
            json!({
                "type": 2,
                "style": 5,
                "label": "Open",
                "url": "https://watchanimeattheoffice.com/"
            })
        );
    }

    #[test]
    fn sections_are_clipped_to_three_children() {
        let mut section = SectionBuilder::new();
        for index in 0..5 {
            section = section.text_display(TextDisplayBuilder::new(format!("part {index}")));
        }

        let json = section
            .accessory(ThumbnailBuilder::new("https://example.com/a.webp"))
            .to_json();
        let components = json["components"].as_array().expect("components");
        assert_eq!(components.len(), 3);
        assert_eq!(components[2]["content"], json!("part 2"));
    }

    #[test]
    fn container_serialises_children_accent_color_and_spoiler() {
        let json = ContainerBuilder::new()
            .text_display(TextDisplayBuilder::new(
                "# You have encountered a wild coyote!",
            ))
            .media_gallery(
                MediaGalleryBuilder::new().item(MediaGalleryItemBuilder::new(
                    "https://websitewithopensourceimages/coyote.webp",
                )),
            )
            .separator(SeparatorBuilder::new().spacing(SeparatorSpacing::Small))
            .accent_color(703_487)
            .spoiler(true)
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 17,
                "accent_color": 703_487,
                "spoiler": true,
                "components": [
                    {"type": 10, "content": "# You have encountered a wild coyote!"},
                    {
                        "type": 12,
                        "items": [{
                            "media": {"url": "https://websitewithopensourceimages/coyote.webp"}
                        }]
                    },
                    {"type": 14, "spacing": 1}
                ]
            })
        );
    }

    #[test]
    fn container_accent_colors_are_clipped_to_twenty_four_bits() {
        let json = ContainerBuilder::new().accent_color(0x1FF_FFFF).to_json();

        assert_eq!(json["accent_color"], json!(0x00FF_FFFF));
    }

    #[test]
    fn file_upload_serialises_every_documented_field() {
        let json = FileUploadBuilder::new("file_upload")
            .min_values(1)
            .max_values(10)
            .required(true)
            .file_type("image")
            .file_type(".png")
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 19,
                "custom_id": "file_upload",
                "min_values": 1,
                "max_values": 10,
                "required": true,
                "file_types": ["image", ".png"]
            })
        );
    }

    #[test]
    fn file_upload_clips_its_documented_limits() {
        let mut upload = FileUploadBuilder::new("c".repeat(150))
            .min_values(0)
            .max_values(99)
            .required(true);
        for index in 0..12 {
            upload = upload.file_type(format!("type{index}"));
        }

        let json = upload.to_json();
        assert_eq!(json["custom_id"], json!("c".repeat(100)));
        // `required` defaults to true, so `min_values` cannot be 0.
        assert_eq!(json["min_values"], json!(1));
        assert_eq!(json["max_values"], json!(10));
        let file_types = json["file_types"].as_array().expect("file types");
        assert_eq!(file_types.len(), 10);

        // A non-required upload may have `min_values: 0`.
        assert_eq!(
            FileUploadBuilder::new("upload")
                .min_values(0)
                .required(false)
                .to_json()["min_values"],
            json!(0)
        );
    }

    #[test]
    fn radio_group_serialises_options() {
        let json = RadioGroupBuilder::new("class_radio")
            .option(GroupOptionBuilder::new("warrior", "Warrior").description("Strong and brave"))
            .option(GroupOptionBuilder::new("rogue", "Rogue").default(true))
            .required(true)
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 21,
                "custom_id": "class_radio",
                "options": [
                    {
                        "value": "warrior",
                        "label": "Warrior",
                        "description": "Strong and brave"
                    },
                    {
                        "value": "rogue",
                        "label": "Rogue",
                        "default": true
                    }
                ],
                "required": true
            })
        );
    }

    #[test]
    fn radio_groups_are_clipped_to_ten_options() {
        let mut radio = RadioGroupBuilder::new("radio");
        for index in 0..12 {
            radio = radio.option(GroupOptionBuilder::new(
                format!("value{index}"),
                format!("Option {index}"),
            ));
        }

        let json = radio.to_json();
        let options = json["options"].as_array().expect("options");
        assert_eq!(options.len(), 10);
        assert_eq!(options[9]["value"], json!("value9"));
    }

    #[test]
    fn checkbox_group_serialises_bounds_and_options() {
        let json = CheckboxGroupBuilder::new("event_checkbox")
            .option(GroupOptionBuilder::new("march-4", "March 4th"))
            .option(GroupOptionBuilder::new("march-7", "March 7th").description("A Saturday"))
            .min_values(1)
            .max_values(5)
            .required(true)
            .to_json();

        assert_eq!(
            json,
            json!({
                "type": 22,
                "custom_id": "event_checkbox",
                "options": [
                    {"value": "march-4", "label": "March 4th"},
                    {
                        "value": "march-7",
                        "label": "March 7th",
                        "description": "A Saturday"
                    }
                ],
                "min_values": 1,
                "max_values": 5,
                "required": true
            })
        );
    }

    #[test]
    fn checkbox_group_clips_its_documented_limits() {
        let mut group = CheckboxGroupBuilder::new("group")
            .min_values(0)
            .max_values(99)
            .required(true);
        for index in 0..12 {
            group = group.option(GroupOptionBuilder::new(
                format!("value{index}"),
                format!("Option {index}"),
            ));
        }

        let json = group.to_json();
        assert_eq!(json["min_values"], json!(1));
        assert_eq!(json["max_values"], json!(10));
        assert_eq!(json["options"].as_array().map(Vec::len), Some(10));

        // A non-required group may have `min_values: 0`, and `max_values` is
        // kept at least at `min_values`.
        let json = CheckboxGroupBuilder::new("group")
            .min_values(0)
            .max_values(0)
            .required(false)
            .to_json();
        assert_eq!(json["min_values"], json!(0));
        assert_eq!(json["max_values"], json!(1));

        let json = CheckboxGroupBuilder::new("group")
            .min_values(5)
            .max_values(2)
            .required(true)
            .to_json();
        assert_eq!(json["min_values"], json!(5));
        assert_eq!(json["max_values"], json!(5));
    }

    #[test]
    fn group_option_strings_are_clipped_to_one_hundred_characters() {
        let json = GroupOptionBuilder::new("v".repeat(150), "l".repeat(150))
            .description("d".repeat(150))
            .to_json();

        for field in ["value", "label", "description"] {
            assert_eq!(
                json[field].as_str().map(str::chars).map(Iterator::count),
                Some(100),
                "{field} is clipped"
            );
        }
    }

    #[test]
    fn checkbox_serialises_default_and_required() {
        assert_eq!(
            CheckboxBuilder::new("like_checkbox").to_json(),
            json!({"type": 23, "custom_id": "like_checkbox"})
        );
        assert_eq!(
            CheckboxBuilder::new("like_checkbox")
                .default(true)
                .required(false)
                .to_json(),
            json!({
                "type": 23,
                "custom_id": "like_checkbox",
                "default": true,
                "required": false
            })
        );
    }

    #[test]
    fn custom_ids_are_clipped_to_one_hundred_characters() {
        assert_eq!(
            RadioGroupBuilder::new("c".repeat(150)).to_json()["custom_id"],
            json!("c".repeat(100))
        );
        assert_eq!(
            CheckboxGroupBuilder::new("c".repeat(150)).to_json()["custom_id"],
            json!("c".repeat(100))
        );
        assert_eq!(
            CheckboxBuilder::new("c".repeat(150)).to_json()["custom_id"],
            json!("c".repeat(100))
        );
    }

    #[test]
    fn message_component_builder_serialises_each_variant() {
        let text = MessageComponentBuilder::from(TextDisplayBuilder::new("hello"));
        assert_eq!(text.kind(), ComponentKind::TextDisplay);
        assert!(text.requires_v2());
        assert_eq!(text.to_json(), json!({"type": 10, "content": "hello"}));

        let row = MessageComponentBuilder::from(
            ActionRowBuilder::new().button(ButtonBuilder::primary("click", "Click me")),
        );
        assert_eq!(row.kind(), ComponentKind::ActionRow);
        assert!(!row.requires_v2());
        assert_eq!(
            row.to_json(),
            json!({
                "type": 1,
                "components": [{"type": 2, "style": 1, "label": "Click me", "custom_id": "click"}]
            })
        );

        let label = MessageComponentBuilder::from(
            LabelBuilder::new("What did you think?")
                .text_input(TextInputBuilder::paragraph("feedback")),
        );
        assert_eq!(label.kind(), ComponentKind::Label);
        assert!(!label.requires_v2());
        assert_eq!(label.to_json()["component"]["type"], json!(4));

        let separator = MessageComponentBuilder::from(SeparatorBuilder::new());
        assert_eq!(separator.kind(), ComponentKind::Separator);
        assert!(separator.requires_v2());
    }

    #[test]
    fn every_documented_component_kind_is_in_the_union() {
        let kinds = [
            MessageComponentBuilder::from(
                ActionRowBuilder::new().button(ButtonBuilder::primary("id", "label")),
            ),
            MessageComponentBuilder::from(ButtonBuilder::primary("id", "label")),
            MessageComponentBuilder::from(StringSelectBuilder::new("id")),
            MessageComponentBuilder::from(TextInputBuilder::new("id", TextInputStyle::Short)),
            MessageComponentBuilder::from(UserSelectBuilder::new("id")),
            MessageComponentBuilder::from(RoleSelectBuilder::new("id")),
            MessageComponentBuilder::from(MentionableSelectBuilder::new("id")),
            MessageComponentBuilder::from(ChannelSelectBuilder::new("id")),
            MessageComponentBuilder::from(SectionBuilder::new()),
            MessageComponentBuilder::from(TextDisplayBuilder::new("text")),
            MessageComponentBuilder::from(ThumbnailBuilder::new("https://example.com/a.webp")),
            MessageComponentBuilder::from(MediaGalleryBuilder::new()),
            MessageComponentBuilder::from(FileBuilder::new("attachment://a.txt")),
            MessageComponentBuilder::from(SeparatorBuilder::new()),
            MessageComponentBuilder::from(ContainerBuilder::new()),
            MessageComponentBuilder::from(LabelBuilder::new("label")),
            MessageComponentBuilder::from(FileUploadBuilder::new("id")),
            MessageComponentBuilder::from(RadioGroupBuilder::new("id")),
            MessageComponentBuilder::from(CheckboxGroupBuilder::new("id")),
            MessageComponentBuilder::from(CheckboxBuilder::new("id")),
        ];

        let kinds: Vec<ComponentKind> = kinds.iter().map(MessageComponentBuilder::kind).collect();
        assert_eq!(
            kinds,
            vec![
                ComponentKind::ActionRow,
                ComponentKind::Button,
                ComponentKind::StringSelect,
                ComponentKind::TextInput,
                ComponentKind::UserSelect,
                ComponentKind::RoleSelect,
                ComponentKind::MentionableSelect,
                ComponentKind::ChannelSelect,
                ComponentKind::Section,
                ComponentKind::TextDisplay,
                ComponentKind::Thumbnail,
                ComponentKind::MediaGallery,
                ComponentKind::File,
                ComponentKind::Separator,
                ComponentKind::Container,
                ComponentKind::Label,
                ComponentKind::FileUpload,
                ComponentKind::RadioGroup,
                ComponentKind::CheckboxGroup,
                ComponentKind::Checkbox,
            ]
        );
    }

    #[test]
    fn serialise_components_clips_to_the_documented_message_limit() {
        let components: Vec<MessageComponentBuilder> = (0..45)
            .map(|index| {
                MessageComponentBuilder::from(TextDisplayBuilder::new(format!("part {index}")))
            })
            .collect();

        let serialised = serialise_components(&components);

        assert_eq!(serialised.len(), 40);
        assert_eq!(serialised[0], json!({"type": 10, "content": "part 0"}));
        assert_eq!(serialised[39], json!({"type": 10, "content": "part 39"}));
    }

    #[test]
    fn the_message_flag_constant_is_components_v2() {
        assert_eq!(IS_COMPONENTS_V2_FLAG, 32_768);
        assert_eq!(MAX_MESSAGE_COMPONENTS, 40);
    }
}
