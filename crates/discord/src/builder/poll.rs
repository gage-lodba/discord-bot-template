//! Builder for the message poll create-request object.
//!
//! A poll is sent inside the create-message / followup / interaction-callback
//! payload as the documented Poll Create Request Object
//! (`docs/poll.md`, Poll Create Request Object). It reuses the documented Poll
//! Media Object for the question and answers and the documented Layout Type.
//!
//! # Caller requirements
//!
//! The builder is infallible and serialises whatever it was given, so it can
//! only clip the documented hard limits: the question is kept to 300
//! characters, an answer text to 55, at most 10 answers are serialised and the
//! duration is clipped to 1-768 hours (32 days). Discord additionally requires
//! **at least two answers** for a poll to be valid; this builder does not
//! inject or reject answers, so a caller that wants a valid request must add
//! two or more.
//!
//! Reference: `docs/poll.md` (Poll Create Request Object, Poll Media Object,
//! Poll Answer Object, Layout Type).

use serde_json::{Map, Value};

use crate::model::emoji::ReactionType;
use crate::model::message::PollLayout;

use super::limits;

/// Maximum characters in a poll question.
///
/// Reference: `docs/poll.md` (Poll Media Object) — "The maximum length of
/// `text` is 300 for the question".
const POLL_QUESTION_TEXT: usize = 300;

/// Maximum characters in one poll answer.
///
/// Reference: `docs/poll.md` (Poll Media Object) — "55 for any answer".
const POLL_ANSWER_TEXT: usize = 55;

/// Maximum answers per poll.
///
/// Reference: `docs/poll.md` (Poll Answer Object) — "Currently, there is a
/// maximum of 10 answers per poll."
const POLL_ANSWERS: usize = 10;

/// Minimum documented poll duration, in hours.
///
/// The reference says the duration is the "Number of hours the poll should be
/// open for, up to 32 days"; zero hours is not a meaningful poll, so it is
/// clipped to one hour.
///
/// Reference: `docs/poll.md` (Poll Create Request Object, `duration`).
const POLL_MIN_DURATION_HOURS: u32 = 1;

/// Maximum documented poll duration, in hours (32 days).
///
/// Reference: `docs/poll.md` (Poll Create Request Object) — "up to 32 days".
const POLL_MAX_DURATION_HOURS: u32 = 32 * 24;

/// One answer collected by [`PollBuilder`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct PollAnswerDraft {
    /// The answer text, clipped to [`POLL_ANSWER_TEXT`] on serialisation.
    text: String,
    /// The optional answer emoji.
    emoji: Option<ReactionType>,
}

/// Builds the Poll Create Request Object of `docs/poll.md`.
///
/// ```
/// use discord::builder::poll::PollBuilder;
///
/// let poll = PollBuilder::new("Deploy to production?")
///     .answer("Yes")
///     .answer("No")
///     .duration_hours(24)
///     .to_json();
///
/// assert_eq!(poll["question"]["text"], "Deploy to production?");
/// assert_eq!(poll["answers"][0]["poll_media"]["text"], "Yes");
/// ```
///
/// Reference: `docs/poll.md` (Poll Create Request Object).
#[derive(Clone, Debug, Default)]
pub struct PollBuilder {
    question: String,
    answers: Vec<PollAnswerDraft>,
    duration_hours: Option<u32>,
    allow_multiselect: Option<bool>,
    layout: PollLayout,
}

impl PollBuilder {
    /// Creates a poll for `question` (at most 300 characters are serialised).
    ///
    /// Reference: `docs/poll.md` (Poll Media Object) — the question only
    /// supports `text`.
    #[must_use]
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            ..Self::default()
        }
    }

    /// Appends a text-only answer (at most 55 characters are serialised).
    ///
    /// Reference: `docs/poll.md` (Poll Media Object, Poll Answer Object).
    #[must_use]
    pub fn answer(mut self, text: impl Into<String>) -> Self {
        self.answers.push(PollAnswerDraft {
            text: text.into(),
            emoji: None,
        });
        self
    }

    /// Appends an answer with an optional emoji.
    ///
    /// The emoji is a partial emoji object (`docs/emoji.md`, Custom Emoji
    /// Examples): `{"name": "🔥"}` for a unicode emoji or `{"id": "..."}`
    /// for a custom one. `docs/poll.md` (Poll Media Object) says an answer
    /// only needs the emoji `id` (custom) or `name` (default); the crate's
    /// [`ReactionType`] emits the documented partial emoji shape.
    ///
    /// Reference: `docs/poll.md` (Poll Media Object, Poll Answer Object).
    #[must_use]
    pub fn answer_with_emoji(
        mut self,
        text: impl Into<String>,
        emoji: impl Into<ReactionType>,
    ) -> Self {
        self.answers.push(PollAnswerDraft {
            text: text.into(),
            emoji: Some(emoji.into()),
        });
        self
    }

    /// Sets how many hours the poll stays open (clipped to 1-768 hours).
    ///
    /// The field is omitted when it is not set, in which case Discord's
    /// documented default of 24 hours applies.
    ///
    /// Reference: `docs/poll.md` (Poll Create Request Object, `duration`).
    #[must_use]
    pub const fn duration_hours(mut self, hours: u32) -> Self {
        self.duration_hours = Some(hours);
        self
    }

    /// Sets whether a user can select multiple answers (Discord defaults to
    /// `false`).
    ///
    /// Reference: `docs/poll.md` (Poll Create Request Object,
    /// `allow_multiselect`).
    #[must_use]
    pub const fn allow_multiselect(mut self, allow_multiselect: bool) -> Self {
        self.allow_multiselect = Some(allow_multiselect);
        self
    }

    /// Sets the poll layout; the documented default is
    /// [`PollLayout::Default`].
    ///
    /// Reference: `docs/poll.md` (Layout Type).
    #[must_use]
    pub const fn layout(mut self, layout: PollLayout) -> Self {
        self.layout = layout;
        self
    }

    /// Serialises the Poll Create Request Object:
    /// `{"question", "answers", "duration"?, "allow_multiselect"?,
    /// "layout_type"?}`.
    ///
    /// The optional fields are omitted unless set; `layout_type` is omitted for
    /// the documented default layout, which Discord also defaults to.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();

        let mut question = Map::new();
        question.insert(
            "text".to_owned(),
            Value::String(limits::truncate(&self.question, POLL_QUESTION_TEXT)),
        );
        object.insert("question".to_owned(), Value::Object(question));

        object.insert(
            "answers".to_owned(),
            Value::Array(
                self.answers
                    .iter()
                    .take(POLL_ANSWERS)
                    .map(PollAnswerDraft::to_json)
                    .collect(),
            ),
        );

        if let Some(hours) = self.duration_hours {
            object.insert(
                "duration".to_owned(),
                Value::from(hours.clamp(POLL_MIN_DURATION_HOURS, POLL_MAX_DURATION_HOURS)),
            );
        }

        if let Some(allow_multiselect) = self.allow_multiselect {
            object.insert(
                "allow_multiselect".to_owned(),
                Value::Bool(allow_multiselect),
            );
        }

        if self.layout != PollLayout::Default {
            object.insert("layout_type".to_owned(), Value::from(self.layout.as_i64()));
        }

        Value::Object(object)
    }
}

impl PollAnswerDraft {
    /// Serialises one documented Poll Answer Object.
    fn to_json(&self) -> Value {
        let mut media = Map::new();
        media.insert(
            "text".to_owned(),
            Value::String(limits::truncate(&self.text, POLL_ANSWER_TEXT)),
        );

        if let Some(emoji) = &self.emoji
            && let Ok(emoji) = serde_json::to_value(emoji)
        {
            media.insert("emoji".to_owned(), emoji);
        }

        let mut answer = Map::new();
        answer.insert("poll_media".to_owned(), Value::Object(media));

        Value::Object(answer)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::id::EmojiId;
    use crate::model::emoji::ReactionType;
    use crate::model::message::PollLayout;

    use super::{POLL_ANSWER_TEXT, POLL_ANSWERS, POLL_QUESTION_TEXT, PollBuilder};

    #[test]
    fn serialises_the_documented_poll_create_request() {
        let poll = PollBuilder::new("Deploy to production?")
            .answer("Yes")
            .answer_with_emoji("No", "❌")
            .duration_hours(48)
            .allow_multiselect(true)
            .to_json();

        assert_eq!(
            poll,
            json!({
                "question": {"text": "Deploy to production?"},
                "answers": [
                    {"poll_media": {"text": "Yes"}},
                    {"poll_media": {"text": "No", "emoji": {"name": "❌"}}}
                ],
                "duration": 48,
                "allow_multiselect": true
            }),
            "layout_type is omitted for the documented default layout"
        );
    }

    #[test]
    fn absent_optional_parts_are_omitted() {
        let poll = PollBuilder::new("Pick one")
            .answer("First")
            .answer("Second")
            .to_json();

        assert_eq!(
            poll,
            json!({
                "question": {"text": "Pick one"},
                "answers": [
                    {"poll_media": {"text": "First"}},
                    {"poll_media": {"text": "Second"}}
                ]
            })
        );
        assert!(poll.get("duration").is_none());
        assert!(poll.get("allow_multiselect").is_none());
        assert_eq!(
            PollBuilder::default().to_json(),
            json!({"question": {"text": ""}, "answers": []})
        );
    }

    #[test]
    fn custom_emoji_serialises_as_a_partial_emoji() {
        let poll = PollBuilder::new("Pick")
            .answer_with_emoji(
                "Blurple",
                ReactionType::Custom {
                    id: EmojiId::new(41_771_983_429_993_937),
                    name: Some("blurple".to_owned()),
                    animated: true,
                },
            )
            .to_json();

        assert_eq!(
            poll["answers"][0]["poll_media"]["emoji"],
            json!({"id": "41771983429993937", "name": "blurple", "animated": true})
        );
    }

    #[test]
    fn question_and_answer_text_are_clipped_to_the_documented_limits() {
        let poll = PollBuilder::new("q".repeat(POLL_QUESTION_TEXT + 50))
            .answer("a".repeat(POLL_ANSWER_TEXT + 50))
            .to_json();

        assert_eq!(
            poll["question"]["text"].as_str().map(str::len),
            Some(POLL_QUESTION_TEXT)
        );
        assert_eq!(
            poll["answers"][0]["poll_media"]["text"]
                .as_str()
                .map(str::len),
            Some(POLL_ANSWER_TEXT)
        );
    }

    #[test]
    fn the_question_is_clipped_by_characters_not_bytes() {
        let poll = PollBuilder::new("🎉".repeat(POLL_QUESTION_TEXT + 10)).to_json();

        assert_eq!(
            poll["question"]["text"]
                .as_str()
                .map(|text| text.chars().count()),
            Some(POLL_QUESTION_TEXT)
        );
    }

    #[test]
    fn at_most_ten_answers_are_serialised() {
        let mut builder = PollBuilder::new("Pick");

        for index in 0..12 {
            builder = builder.answer(format!("answer {index}"));
        }

        let poll = builder.to_json();

        assert_eq!(poll["answers"].as_array().map(Vec::len), Some(POLL_ANSWERS));
    }

    #[test]
    fn fewer_than_two_answers_are_kept_for_the_caller_to_notice() {
        // Discord requires at least two answers for a valid poll; the
        // infallible builder documents that as a caller requirement instead of
        // rejecting or inventing answers.
        let poll = PollBuilder::new("Pick").answer("Only one").to_json();

        assert_eq!(poll["answers"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            PollBuilder::new("Empty").to_json()["answers"],
            json!([]),
            "an empty answer list serialises as an empty array"
        );
    }

    #[test]
    fn duration_is_clipped_to_the_documented_window() {
        assert_eq!(
            PollBuilder::new("q").duration_hours(0).to_json()["duration"],
            json!(1)
        );
        assert_eq!(
            PollBuilder::new("q").duration_hours(24).to_json()["duration"],
            json!(24)
        );
        assert_eq!(
            PollBuilder::new("q").duration_hours(1_000).to_json()["duration"],
            json!(768),
            "32 days is 768 hours"
        );
    }

    #[test]
    fn a_future_layout_value_is_sent_explicitly() {
        let poll = PollBuilder::new("q")
            .layout(PollLayout::Unknown(7))
            .to_json();

        assert_eq!(poll["layout_type"], json!(7));

        let default = PollBuilder::new("q").layout(PollLayout::Default).to_json();
        assert!(default.get("layout_type").is_none());
    }
}
