//! Payload builders for outbound requests.
//!
//! One submodule per payload family: application commands, messages and embeds,
//! interaction responses, and presence updates. Builders validate the parts the
//! API reference gives hard limits for (for example the 2000-character message
//! content limit and the 10-embed limit) where that is cheap to do, and
//! serialise themselves to the documented JSON shape.
//!
//! The contract makes the serialisation methods infallible (`to_json` returns a
//! [`serde_json::Value`]), so documented limits are enforced by truncating the
//! input instead of returning an error: a message `content` longer than 2000
//! characters, more than 10 embeds, more than 25 command options, and so on are
//! clipped to the documented maximum. Callers that need strict validation can
//! measure their input first.
//!
//! Reference: `docs/message.md` (Create Message, Embed Object, Embed Limits),
//! `docs/gateway-events.md` (Update Presence, Activity Object) and
//! `docs/components-reference.md` (buttons and their partial emoji object).

pub mod attachment;
pub mod command;
pub mod component;
pub mod embed;
pub mod interaction;
pub mod message;
pub mod poll;
pub mod presence;

/// Resource limits from the API reference that the builders enforce.
///
/// The limits are listed in the pages cited on each constant; the contract's
/// infallible `to_json` methods clip values that exceed them.
pub(crate) mod limits {
    /// Maximum characters in a message's `content`.
    ///
    /// Reference: `docs/message.md` (Create Message, `content`).
    pub(crate) const MESSAGE_CONTENT: usize = 2000;

    /// Maximum rich embeds per message.
    ///
    /// Reference: `docs/message.md` (Create Message, `embeds`).
    pub(crate) const EMBEDS_PER_MESSAGE: usize = 10;

    /// Maximum top-level options on an application command.
    ///
    /// Reference: `docs/application-commands.md` (Application Command Structure, "max of 25").
    pub(crate) const COMMAND_OPTIONS: usize = 25;

    /// Maximum choices on one application command option.
    ///
    /// Reference: `docs/application-commands.md` (Application Command Option
    /// Structure, "max 25").
    pub(crate) const COMMAND_CHOICES: usize = 25;

    /// Maximum characters in one choice name.
    ///
    /// Reference: `docs/application-commands.md` (Application Command Option
    /// Choice Structure, "1-100 character choice name").
    pub(crate) const COMMAND_CHOICE_NAME: usize = 100;

    /// Maximum characters in one string choice value.
    ///
    /// Reference: `docs/application-commands.md` (Application Command Option
    /// Choice Structure, "up to 100 characters if string").
    pub(crate) const COMMAND_CHOICE_VALUE: usize = 100;

    /// Maximum buttons in one action row.
    ///
    /// Reference: `docs/components-reference.md` (Action Row, "Up to 5
    /// contextually grouped buttons").
    pub(crate) const BUTTONS_PER_ROW: usize = 5;

    /// Maximum action rows in one message, and therefore the maximum number of
    /// rows the interaction reply builder will serialise (5 rows x 5 buttons).
    ///
    /// Reference: `docs/components-reference.md` (Action Row) and
    /// `docs/receiving-and-responding.md` (Interaction Callback Data Structure,
    /// Messages).
    pub(crate) const ACTION_ROWS: usize = 5;

    /// Maximum characters in an embed title.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_TITLE: usize = 256;

    /// Maximum characters in an embed description.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_DESCRIPTION: usize = 4096;

    /// Maximum fields in an embed.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_FIELDS: usize = 25;

    /// Maximum characters in an embed field name.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_FIELD_NAME: usize = 256;

    /// Maximum characters in an embed field value.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_FIELD_VALUE: usize = 1024;

    /// Maximum characters in an embed footer text.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_FOOTER_TEXT: usize = 2048;

    /// Maximum characters in an embed author name.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_AUTHOR_NAME: usize = 256;

    /// Maximum total characters across the title, description, field name,
    /// field value, footer text and author name of all embeds in a message.
    ///
    /// Reference: `docs/message.md` (Embed Limits).
    pub(crate) const EMBED_TOTAL: usize = 6000;

    /// Maximum characters in a component `custom_id` (1-100).
    ///
    /// Reference: `docs/components-reference.md` (Anatomy of a Component,
    /// Custom ID).
    pub(crate) const CUSTOM_ID: usize = 100;

    /// Maximum characters in a button label.
    ///
    /// Reference: `docs/components-reference.md` (Button Structure).
    pub(crate) const BUTTON_LABEL: usize = 80;

    /// Maximum characters in a link button url.
    ///
    /// Reference: `docs/components-reference.md` (Button Structure).
    pub(crate) const BUTTON_URL: usize = 512;

    /// Maximum characters in a select menu placeholder.
    ///
    /// Reference: `docs/components-reference.md` (String Select Structure).
    pub(crate) const SELECT_PLACEHOLDER: usize = 150;

    /// Maximum characters in a select option's `label`, `value` and
    /// `description`.
    ///
    /// Reference: `docs/components-reference.md` (Select Option Structure).
    pub(crate) const SELECT_OPTION_TEXT: usize = 100;

    /// Maximum choices in a string select.
    ///
    /// Reference: `docs/components-reference.md` (String Select Structure).
    pub(crate) const SELECT_OPTIONS: usize = 25;

    /// Maximum number of values a select can be configured for.
    ///
    /// Reference: `docs/components-reference.md` (String Select Structure).
    pub(crate) const SELECT_MAX_VALUES: u32 = 25;

    /// Maximum characters in a text input placeholder.
    ///
    /// Reference: `docs/components-reference.md` (Text Input Structure).
    pub(crate) const TEXT_INPUT_PLACEHOLDER: usize = 100;

    /// Maximum characters in a text input `value` or length bound.
    ///
    /// Reference: `docs/components-reference.md` (Text Input Structure).
    pub(crate) const TEXT_INPUT_LENGTH: u32 = 4000;

    /// Maximum characters in a modal title.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Response
    /// Object, Modal).
    pub(crate) const MODAL_TITLE: usize = 45;

    /// Maximum components in a modal (1-5).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Response
    /// Object, Modal).
    pub(crate) const MODAL_COMPONENTS: usize = 5;

    /// Maximum text inputs in an action row in a legacy modal.
    ///
    /// Reference: `docs/components-reference.md` (Action Row) and
    /// `docs/receiving-and-responding.md` (Modal).
    pub(crate) const TEXT_INPUTS_PER_ROW: usize = 5;

    /// Maximum characters in a label's `label` text.
    ///
    /// Reference: `docs/components-reference.md` (Label Structure).
    pub(crate) const LABEL_TEXT: usize = 45;

    /// Maximum characters in a label's `description`.
    ///
    /// Reference: `docs/components-reference.md` (Label Structure).
    pub(crate) const LABEL_DESCRIPTION: usize = 100;

    /// Maximum autocomplete choices in a callback.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
    /// Structure, Autocomplete: "max of 25 choices").
    pub(crate) const AUTOCOMPLETE_CHOICES: usize = 25;

    /// Maximum characters in an autocomplete choice's `name`.
    ///
    /// Reference: `docs/application-commands.md` (Application Command Option
    /// Choice Structure).
    pub(crate) const CHOICE_NAME: usize = 100;

    /// Returns `text` clipped to at most `max` characters.
    pub(crate) fn truncate(text: &str, max: usize) -> String {
        if text.chars().count() <= max {
            text.to_owned()
        } else {
            text.chars().take(max).collect()
        }
    }

    /// Clips a JSON string to at most `max` characters and subtracts the kept
    /// length from `budget`, never allowing the budget to go below zero.
    ///
    /// Used to enforce the combined 6000-character embed limit across a whole
    /// message without re-formatting the parts the model has already
    /// serialised.
    pub(crate) fn clamp_json_string(text: &mut String, max: usize, budget: &mut usize) {
        let allowed = max.min(*budget);
        let length = text.chars().count();

        if length > allowed {
            *text = text.chars().take(allowed).collect();
            *budget -= allowed;
        } else {
            *budget -= length;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::limits;

    #[test]
    fn truncate_counts_characters_not_bytes() {
        assert_eq!(limits::truncate("aé🎉", 2), "aé");
        assert_eq!(limits::truncate("short", 10), "short");
    }

    #[test]
    fn clamp_json_string_spends_the_budget() {
        let mut budget = 5;
        let mut text = String::from("abcdefg");

        limits::clamp_json_string(&mut text, 100, &mut budget);

        assert_eq!(text, "abcde");
        assert_eq!(budget, 0);
    }

    #[test]
    fn clamp_json_string_never_goes_below_zero() {
        let mut budget = 2;
        let mut text = String::from("abc");

        limits::clamp_json_string(&mut text, 100, &mut budget);

        assert_eq!(text, "ab");
        assert_eq!(budget, 0);

        let mut empty = String::from("abc");
        limits::clamp_json_string(&mut empty, 100, &mut budget);

        assert_eq!(empty, "");
        assert_eq!(budget, 0);
    }
}
