use std::fmt;

use discord::builder::interaction::InteractionEditBuilder;
use discord::builder::interaction::ReplyBuilder;
use discord::builder::message::MessageBuilder;
use discord::client::Context;
use discord::error::Error as DiscordError;
use discord::error::HttpError;
use discord::model::application::CommandInteraction;
use discord::model::application::ComponentInteraction;

use tracing::error;
use tracing::warn;

/// Reply used when an interaction names a `custom_id` the bot does not handle.
///
/// Answering is what keeps Discord from showing the user "This interaction
/// failed". A stale button or modal is a normal situation, not a fault.
///
/// Reference: `docs/components-reference.md` (Anatomy of a Component, Custom
/// ID) and `docs/receiving-and-responding.md` (Interaction Callback).
pub(crate) const UNSUPPORTED_MESSAGE: &str = "This action is no longer supported.";

/// Error type shared by the command layer.
///
/// It is deliberately small: every variant is either a borrowed string or a
/// handful of bytes, so `Result<(), AppError>` stays well under clippy's
/// `large-error-threshold`. That matters because every command's `run()`
/// returns this type and CI denies warnings.
#[derive(Debug)]
pub(crate) enum AppError {
    /// A Discord operation failed.
    Discord(DiscordError),
    /// The interaction was missing an option the command cannot work without.
    MissingOption(&'static str),
    /// The interaction supplied an option with an unusable type.
    InvalidOption {
        option: &'static str,
        expected: &'static str,
    },
    /// The command cannot proceed for a reason worth showing to the user.
    Denied(&'static str),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discord(inner) => write!(f, "{inner}"),
            Self::MissingOption(option) => write!(f, "missing required option '{option}'"),
            Self::InvalidOption { option, expected } => {
                write!(f, "option '{option}' should be {expected}")
            }
            Self::Denied(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Discord(inner) => Some(inner),
            _ => None,
        }
    }
}

impl From<DiscordError> for AppError {
    fn from(value: DiscordError) -> Self {
        Self::Discord(value)
    }
}

impl AppError {
    /// Returns true when this is a normal outcome rather than a fault.
    ///
    /// A refused request ([`Self::Denied`]) is a normal outcome, so it is logged
    /// at `warn!` rather than `error!`. Real faults stay at `error!` so they are
    /// not buried among expected refusals.
    #[must_use]
    pub(crate) fn is_expected(&self) -> bool {
        matches!(self, Self::Denied(_))
    }

    /// Logs this error at the level its nature warrants.
    fn log(&self, context: &str, command: &str) {
        if self.is_expected() {
            warn!("{context} (command '{command}'): {self}");
        } else {
            error!("{context} (command '{command}'): {self}");
        }
    }
}

/// Logs a failure and, when possible, tells the user about it.
///
/// A failure must never be silent: the operator gets a log line and the user
/// gets a message rather than Discord's bare "application did not respond"
/// timeout.
///
/// The caller does not have to say whether the command deferred. Discord rejects
/// a second acknowledgement with code [`INTERACTION_ALREADY_ACKNOWLEDGED`], and
/// that specific rejection means the right move is to complete the deferred reply
/// with `edit_response` instead. Any other failure is surfaced as-is rather than
/// overwriting a message the user may already have received.
///
/// Reference: `docs/opcodes-and-status-codes.md` (JSON Error Codes) and
/// `docs/reference.md` (Error Messages).
pub(crate) async fn report(
    ctx: &Context,
    interaction: &CommandInteraction,
    context: &str,
    err: AppError,
) {
    err.log(context, &interaction.data.name);

    let text = format!("Something went wrong: {err}");

    let response = ReplyBuilder::new().content(text.clone()).ephemeral();

    let Err(respond_err) = interaction.respond(&ctx.http, response).await else {
        return;
    };

    // The command deferred before failing, so finish the reply the user is
    // looking at instead of leaving them on a permanent "thinking..." placeholder.
    if is_already_acknowledged(&respond_err) {
        let edit = InteractionEditBuilder::new().content(text);

        if let Err(edit_err) = interaction.edit_response(&ctx.http, edit).await {
            error!(
                "failed to report error for command '{}': {}",
                interaction.data.name, edit_err
            );
        }

        return;
    }

    error!(
        "failed to report error for command '{}': {}",
        interaction.data.name, respond_err
    );
}

/// Logs a failure of a component interaction and, when possible, tells the user
/// about it.
///
/// The component counterpart of [`report`]: a failed button click must not leave
/// the user with Discord's bare "This interaction failed". If the interaction was
/// already acknowledged (for example the update itself succeeded but a later
/// edit did not), the message is sent as an ephemeral followup instead of
/// attempting a second acknowledgement.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback, Followup
/// Messages) and `docs/opcodes-and-status-codes.md` (JSON Error Codes).
pub(crate) async fn report_component(
    ctx: &Context,
    interaction: &ComponentInteraction,
    context: &str,
    err: AppError,
) {
    err.log(context, &interaction.data.custom_id);

    let text = format!("Something went wrong: {err}");
    let response = ReplyBuilder::new().content(text.clone()).ephemeral();

    let Err(respond_err) = interaction.respond(&ctx.http, response).await else {
        return;
    };

    // The component was already acknowledged earlier in the handler, so a
    // second callback would be rejected. Say it in a followup instead.
    if is_already_acknowledged(&respond_err) {
        let followup = MessageBuilder::new().content(text).ephemeral();

        if let Err(followup_err) = interaction.followup(&ctx.http, &followup).await {
            error!(
                "failed to report an error for component interaction '{}': {}",
                interaction.data.custom_id, followup_err
            );
        }

        return;
    }

    error!(
        "failed to report an error for component interaction '{}': {}",
        interaction.data.custom_id, respond_err
    );
}

/// Discord's "Interaction has already been acknowledged" error code.
const INTERACTION_ALREADY_ACKNOWLEDGED: i64 = 40060;

/// Returns true when a failed acknowledgement means the interaction was already
/// answered, so the deferred reply must be completed instead.
///
/// Split from [`is_already_acknowledged`] so the decision itself is testable
/// without constructing a full [`DiscordError`] around an HTTP failure.
fn is_already_acknowledged_response(status: u16, error_code: i64) -> bool {
    status == 400 && error_code == INTERACTION_ALREADY_ACKNOWLEDGED
}

/// Returns true when the failure means the interaction was already answered.
fn is_already_acknowledged(err: &DiscordError) -> bool {
    match err {
        DiscordError::Http(HttpError::Unsuccessful {
            status,
            code: Some(code),
            ..
        }) => is_already_acknowledged_response(*status, *code),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use discord::error::GatewayError;

    use super::*;

    /// Builds an unsuccessful HTTP error with the given status and Discord code.
    fn http_error(status: u16, code: i64) -> DiscordError {
        DiscordError::Http(HttpError::Unsuccessful {
            status,
            code: Some(code),
            message: "test".to_owned(),
        })
    }

    #[test]
    fn already_acknowledged_is_detected_by_error_code() {
        // 40060 means the command deferred before failing, so `report` must
        // finish the deferred reply instead of leaving a "thinking..." placeholder.
        assert!(is_already_acknowledged_response(400, 40060));
        assert!(is_already_acknowledged(&http_error(400, 40060)));
    }

    #[test]
    fn other_error_codes_are_not_already_acknowledged() {
        // Guards the fallback against firing on an unrelated failure and
        // overwriting a reply the user may already have received.
        assert!(!is_already_acknowledged_response(400, 40001));
        assert!(!is_already_acknowledged_response(400, 10062));
        assert!(!is_already_acknowledged_response(400, 50035));
        assert!(!is_already_acknowledged(&http_error(400, 40001)));
    }

    #[test]
    fn a_non_400_status_is_not_already_acknowledged() {
        // 40060 is only meaningful on a 400; a 403 or 429 carrying the same code
        // must not be treated as "already answered".
        assert!(!is_already_acknowledged_response(403, 40060));
        assert!(!is_already_acknowledged_response(429, 40060));
        assert!(!is_already_acknowledged_response(200, 40060));
    }

    #[test]
    fn errors_without_a_discord_code_are_not_treated_as_already_acknowledged() {
        let no_code = DiscordError::Http(HttpError::Unsuccessful {
            status: 400,
            code: None,
            message: "bad request".to_owned(),
        });

        assert!(!is_already_acknowledged(&no_code));
    }

    #[test]
    fn non_http_errors_are_not_treated_as_already_acknowledged() {
        // A transport error, a gateway error and a local I/O error must all
        // report false: none of them is evidence that the user was answered.
        assert!(!is_already_acknowledged(&DiscordError::Http(
            HttpError::Transport("connection reset".to_owned())
        )));
        assert!(!is_already_acknowledged(&DiscordError::Gateway(
            GatewayError::Protocol("unexpected opcode".to_owned())
        )));
        assert!(!is_already_acknowledged(&DiscordError::Io(
            std::io::Error::other("boom")
        )));
    }

    #[test]
    fn missing_option_message_names_the_option() {
        let err = AppError::MissingOption("message");
        assert_eq!(err.to_string(), "missing required option 'message'");
    }

    #[test]
    fn invalid_option_message_explains_the_expected_type() {
        let err = AppError::InvalidOption {
            option: "target",
            expected: "a user",
        };
        assert_eq!(err.to_string(), "option 'target' should be a user");
    }

    #[test]
    fn denied_message_passes_through() {
        let err = AppError::Denied("You do not have permission to use this command.");
        assert_eq!(
            err.to_string(),
            "You do not have permission to use this command."
        );
    }

    #[test]
    fn discord_errors_convert_and_keep_their_message() {
        let err: AppError = DiscordError::Http(HttpError::Transport("boom".to_owned())).into();

        assert_eq!(err.to_string(), "request failed: boom");
        assert!(std::error::Error::source(&err).is_some());
    }

    /// Guards the `clippy::result_large_err` constraint on the command layer.
    ///
    /// Clippy's `large-error-threshold` is 128 bytes, and `Result<(), AppError>`
    /// is as large as `AppError` because `()` is zero sized. Every command
    /// `run()` returns this type under `-D warnings`, so the enum must stay under
    /// the threshold; keeping the Discord error inline (it is small, since its
    /// own heavy variants are boxed by the crate) is what makes that true.
    #[test]
    fn error_type_stays_under_clippy_large_error_threshold() {
        const CLIPPY_LARGE_ERROR_THRESHOLD: usize = 128;

        let app_error_size = size_of::<AppError>();
        let discord_error_size = size_of::<DiscordError>();

        assert!(
            app_error_size < CLIPPY_LARGE_ERROR_THRESHOLD,
            "AppError is {app_error_size} bytes, which trips clippy::result_large_err at the \
             {CLIPPY_LARGE_ERROR_THRESHOLD} byte threshold"
        );
        assert!(
            discord_error_size < CLIPPY_LARGE_ERROR_THRESHOLD,
            "discord::Error is {discord_error_size} bytes; consider boxing a variant"
        );
    }
}
