//! Embed builders.
//!
//! The builder produces the crate's model [`Embed`] so that outbound payloads
//! and inbound messages share one shape ([`crate::model::message`]). The
//! documented limits from `docs/message.md` (Embed Limits) are applied in
//! [`EmbedBuilder::build`]: a title longer than 256 characters is clipped, a
//! description longer than 4096 characters is clipped, at most 25 fields are
//! kept, and the per-field limits are clipped too.
//!
//! Reference: `docs/message.md` (Embed Object, Embed Limits).

use serde_json::Value;

use crate::model::message::{Embed, EmbedAuthor, EmbedField, EmbedFooter};
use crate::timestamp::Timestamp;

use super::limits;

/// Builds a rich [`Embed`].
///
/// All setters take and return `Self`, so a builder is usually written as one
/// chained expression and finished with [`Self::build`].
///
/// ```
/// use discord::builder::embed::{EmbedBuilder, EmbedFieldBuilder};
/// use discord::model::message::Embed;
///
/// let embed: Embed = EmbedBuilder::new()
///     .title("Status")
///     .description("All systems operational.")
///     .field(EmbedFieldBuilder::new("Latency", "42 ms", true))
///     .build();
///
/// assert_eq!(embed.title.as_deref(), Some("Status"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct EmbedBuilder {
    title: Option<String>,
    description: Option<String>,
    url: Option<String>,
    color: Option<u32>,
    timestamp: Option<Timestamp>,
    author: Option<EmbedAuthorBuilder>,
    footer: Option<EmbedFooterBuilder>,
    fields: Vec<EmbedFieldBuilder>,
}

impl EmbedBuilder {
    /// Creates an empty embed.
    ///
    /// Discord requires at least one embed field to be useful; the request
    /// itself only needs one of `content`/`embeds`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            title: None,
            description: None,
            url: None,
            color: None,
            timestamp: None,
            author: None,
            footer: None,
            fields: Vec::new(),
        }
    }

    /// Sets the embed title (maximum 256 characters).
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the embed description (maximum 4096 characters).
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the URL the embed title links to.
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Sets the accent colour as a 24-bit RGB integer.
    #[must_use]
    pub const fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the embed timestamp.
    ///
    /// Reference: `docs/message.md` (Embed Structure, `timestamp`).
    #[must_use]
    pub const fn timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    /// Sets the embed author.
    #[must_use]
    pub fn author(mut self, author: EmbedAuthorBuilder) -> Self {
        self.author = Some(author);
        self
    }

    /// Sets the embed footer.
    #[must_use]
    pub fn footer(mut self, footer: EmbedFooterBuilder) -> Self {
        self.footer = Some(footer);
        self
    }

    /// Appends a field (up to 25 fields are kept).
    #[must_use]
    pub fn field(mut self, field: EmbedFieldBuilder) -> Self {
        self.fields.push(field);
        self
    }

    /// Builds the model embed, clipping the parts that exceed the documented
    /// limits in `docs/message.md` (Embed Limits).
    ///
    /// The document also says leading and trailing whitespace is trimmed
    /// automatically, so the builder trims it before measuring.
    #[must_use]
    pub fn build(self) -> Embed {
        Embed {
            title: self
                .title
                .as_deref()
                .map(|title| limit_text(title, limits::EMBED_TITLE)),
            description: self
                .description
                .as_deref()
                .map(|description| limit_text(description, limits::EMBED_DESCRIPTION)),
            url: self.url,
            color: self.color,
            timestamp: self.timestamp,
            author: self.author.map(EmbedAuthorBuilder::build),
            footer: self.footer.map(EmbedFooterBuilder::build),
            fields: self
                .fields
                .into_iter()
                .take(limits::EMBED_FIELDS)
                .map(EmbedFieldBuilder::build)
                .collect(),
            ..Embed::default()
        }
    }
}

impl From<EmbedBuilder> for Embed {
    /// Builds the model embed, so `MessageBuilder::embed` and
    /// `ReplyBuilder::embed` accept an [`EmbedBuilder`] directly.
    fn from(builder: EmbedBuilder) -> Self {
        builder.build()
    }
}

/// Builds the author block of an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Author Structure).
#[derive(Clone, Debug)]
pub struct EmbedAuthorBuilder {
    name: String,
    url: Option<String>,
    icon_url: Option<String>,
}

impl EmbedAuthorBuilder {
    /// Creates an author block with the given display name (maximum 256
    /// characters).
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: None,
            icon_url: None,
        }
    }

    /// Sets the URL the author name links to (http/https only).
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Sets the author icon URL (http/https or an attachment URL).
    #[must_use]
    pub fn icon_url(mut self, url: impl Into<String>) -> Self {
        self.icon_url = Some(url.into());
        self
    }

    /// Builds the model author block.
    #[must_use]
    pub fn build(self) -> EmbedAuthor {
        EmbedAuthor {
            name: limit_text(&self.name, limits::EMBED_AUTHOR_NAME),
            url: self.url,
            icon_url: self.icon_url,
            ..EmbedAuthor::default()
        }
    }
}

/// Builds the footer block of an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Footer Structure).
#[derive(Clone, Debug)]
pub struct EmbedFooterBuilder {
    text: String,
    icon_url: Option<String>,
}

impl EmbedFooterBuilder {
    /// Creates a footer with the given text (maximum 2048 characters).
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            icon_url: None,
        }
    }

    /// Sets the footer icon URL (http/https or an attachment URL).
    #[must_use]
    pub fn icon_url(mut self, url: impl Into<String>) -> Self {
        self.icon_url = Some(url.into());
        self
    }

    /// Builds the model footer block.
    #[must_use]
    pub fn build(self) -> EmbedFooter {
        EmbedFooter {
            text: limit_text(&self.text, limits::EMBED_FOOTER_TEXT),
            icon_url: self.icon_url,
            ..EmbedFooter::default()
        }
    }
}

/// Builds one field of an [`Embed`].
///
/// Reference: `docs/message.md` (Embed Field Structure).
#[derive(Clone, Debug)]
pub struct EmbedFieldBuilder {
    name: String,
    value: String,
    inline: bool,
}

impl EmbedFieldBuilder {
    /// Creates a field.
    ///
    /// `inline` controls whether the field is displayed on the same line as
    /// the previous field; it is serialised as the field's `inline` boolean.
    /// The field name is limited to 256 characters and the value to 1024.
    #[must_use]
    pub fn new(name: impl Into<String>, value: impl Into<String>, inline: bool) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            inline,
        }
    }

    /// Builds the model field.
    #[must_use]
    pub fn build(self) -> EmbedField {
        EmbedField {
            name: limit_text(&self.name, limits::EMBED_FIELD_NAME),
            value: limit_text(&self.value, limits::EMBED_FIELD_VALUE),
            inline: Some(self.inline),
        }
    }
}

/// Clips `text` to `max` characters after trimming the ends.
///
/// `docs/message.md` (Embed Limits) says the limits are measured inclusively
/// and that leading/trailing whitespace is trimmed automatically.
fn limit_text(text: &str, max: usize) -> String {
    limits::truncate(text.trim(), max)
}

/// Serialises up to ten embeds, applying the documented per-embed limits and
/// the combined 6000-character limit across the whole message.
///
/// Reference: `docs/message.md` (Create Message, Embed Limits).
pub(crate) fn serialise_embeds(embeds: &[Embed]) -> Vec<Value> {
    let mut budget = limits::EMBED_TOTAL;

    embeds
        .iter()
        .take(limits::EMBEDS_PER_MESSAGE)
        .filter_map(|embed| {
            let mut value = serde_json::to_value(embed).ok()?;
            limit_embed_value(&mut value, &mut budget);
            Some(value)
        })
        .collect()
}

/// Applies the documented embed limits to an already serialised embed.
fn limit_embed_value(value: &mut Value, budget: &mut usize) {
    let Some(object) = value.as_object_mut() else {
        return;
    };

    for (key, max) in [
        ("title", limits::EMBED_TITLE),
        ("description", limits::EMBED_DESCRIPTION),
    ] {
        limit_string_field(object, key, max, budget);
    }

    if let Some(Value::Array(fields)) = object.get_mut("fields") {
        fields.truncate(limits::EMBED_FIELDS);

        for field in fields {
            if let Some(field) = field.as_object_mut() {
                limit_string_field(field, "name", limits::EMBED_FIELD_NAME, budget);
                limit_string_field(field, "value", limits::EMBED_FIELD_VALUE, budget);
            }
        }
    }

    for (key, text_key, max) in [
        ("footer", "text", limits::EMBED_FOOTER_TEXT),
        ("author", "name", limits::EMBED_AUTHOR_NAME),
    ] {
        if let Some(Value::Object(block)) = object.get_mut(key) {
            limit_string_field(block, text_key, max, budget);
        }
    }
}

/// Clips one JSON string to its per-field limit and spends the shared budget.
fn limit_string_field(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    max: usize,
    budget: &mut usize,
) {
    if let Some(Value::String(text)) = object.get_mut(key) {
        limits::clamp_json_string(text, max, budget);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::timestamp::Timestamp;

    use super::{EmbedAuthorBuilder, EmbedBuilder, EmbedFieldBuilder, EmbedFooterBuilder};

    fn serialise(embed: crate::model::message::Embed) -> serde_json::Value {
        serde_json::to_value(embed).expect("an embed always serialises")
    }

    #[test]
    fn build_produces_the_documented_embed_shape() {
        let embed = EmbedBuilder::new()
            .title("Status")
            .description("All systems operational.")
            .url("https://example.com/status")
            .color(0x58_65_F2)
            .timestamp(Timestamp::from_unix_millis(1_437_152_400_000))
            .author(
                EmbedAuthorBuilder::new("Example Bot")
                    .url("https://example.com")
                    .icon_url("https://example.com/avatar.png"),
            )
            .footer(
                EmbedFooterBuilder::new("Updated just now")
                    .icon_url("https://example.com/clock.png"),
            )
            .field(EmbedFieldBuilder::new("Latency", "42 ms", true))
            .field(EmbedFieldBuilder::new("Uptime", "99.99%", false))
            .build();

        assert_eq!(
            serialise(embed),
            json!({
                "title": "Status",
                "description": "All systems operational.",
                "url": "https://example.com/status",
                "color": 5_793_266,
                "timestamp": "2015-07-17T17:00:00.000+00:00",
                "author": {
                    "name": "Example Bot",
                    "url": "https://example.com",
                    "icon_url": "https://example.com/avatar.png"
                },
                "footer": {
                    "text": "Updated just now",
                    "icon_url": "https://example.com/clock.png"
                },
                "fields": [
                    {"name": "Latency", "value": "42 ms", "inline": true},
                    {"name": "Uptime", "value": "99.99%", "inline": false}
                ]
            })
        );
    }

    #[test]
    fn an_embed_never_sets_the_type_field() {
        let value = serialise(EmbedBuilder::new().title("T").build());

        assert!(value.get("type").is_none());
        assert!(value.get("fields").is_none());
        assert_eq!(value, json!({"title": "T"}));
    }

    #[test]
    fn documented_limits_are_clipped() {
        let embed = EmbedBuilder::new()
            .title("t".repeat(300))
            .description("d".repeat(5_000))
            .author(EmbedAuthorBuilder::new("a".repeat(300)))
            .footer(EmbedFooterBuilder::new("f".repeat(3_000)))
            .field(EmbedFieldBuilder::new(
                "n".repeat(300),
                "v".repeat(2_000),
                true,
            ))
            .field(EmbedFieldBuilder::new("kept", "kept", false))
            .build();

        assert_eq!(embed.title.as_deref().map(str::len), Some(256));
        assert_eq!(embed.description.as_deref().map(str::len), Some(4096));
        assert_eq!(
            embed.author.as_ref().map(|author| author.name.len()),
            Some(256)
        );
        assert_eq!(
            embed.footer.as_ref().map(|footer| footer.text.len()),
            Some(2048)
        );

        let value = serialise(embed);
        assert_eq!(value["fields"][0]["name"].as_str().map(str::len), Some(256));
        assert_eq!(
            value["fields"][0]["value"].as_str().map(str::len),
            Some(1024)
        );
    }

    #[test]
    fn at_most_twenty_five_fields_are_kept() {
        let mut builder = EmbedBuilder::new();

        for index in 0..30 {
            builder = builder.field(EmbedFieldBuilder::new(
                format!("field{index}"),
                "value",
                false,
            ));
        }

        let value = serialise(builder.build());
        let fields = value["fields"].as_array().expect("fields array");

        assert_eq!(fields.len(), 25);
        assert_eq!(fields[24]["name"], json!("field24"));
    }

    #[test]
    fn leading_and_trailing_whitespace_is_trimmed() {
        let value = serialise(
            EmbedBuilder::new()
                .title("  Status  ")
                .description("\nAll good\n")
                .build(),
        );

        assert_eq!(value["title"], json!("Status"));
        assert_eq!(value["description"], json!("All good"));
    }

    #[test]
    fn builder_default_matches_new() {
        assert_eq!(
            serialise(EmbedBuilder::default().build()),
            serialise(EmbedBuilder::new().build())
        );
    }
}
