//! Gateway close-code classification.
//!
//! When a gateway connection closes it carries a close code. The reference
//! separates codes that mean "reconnect" from codes that mean "do not reconnect"
//! (the token, intents or API version must be fixed first), and defines the
//! client-initiated `1000`/`1001` codes as a clean shutdown.
//!
//! Reference: `docs/opcodes-and-status-codes.md` (Gateway Close Event Codes),
//! `docs/gateway.md` (Disconnecting, Resuming, Initiating a Disconnect).

use crate::error::GatewayError;

/// Close code used when a heartbeat is not acknowledged before the next beat.
///
/// The reference says the client should terminate a "zombied" connection with
/// any close code besides `1000` or `1001`; `4000` (unknown error) is a valid
/// client-chosen code in the application range.
///
/// Reference: `docs/gateway.md` (Sending Heartbeats).
pub const ZOMBIE_CLOSE_CODE: u16 = 4000;

/// What the client should do after a gateway connection closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// The connection was shut down cleanly; do not reconnect.
    Normal,
    /// Reconnect to the gateway. `resume` says whether the previous session may
    /// be resumed, or whether a fresh Identify is required instead.
    Reconnect {
        /// Whether the client should send Resume (op 6) on the new connection.
        resume: bool,
    },
    /// Reconnecting cannot help: the token, intents, shard or API version must
    /// be corrected first. `start` returns [`GatewayError::Closed`] with
    /// `fatal: true`.
    Fatal,
}

/// Classifies a gateway close code.
///
/// Codes `4004`, `4010`, `4011`, `4012`, `4013` and `4014` are fatal per the
/// reference's "Reconnect" column. `1000`/`1001` are normal client-initiated
/// closes. `4007` (invalid `seq`) and `4009` (session timed out) require a new
/// session, so they reconnect with `resume: false`; every other code reconnects
/// and resumes.
///
/// Reference: `docs/opcodes-and-status-codes.md` (Gateway Close Event Codes).
#[must_use]
pub const fn classify(code: u16) -> CloseAction {
    match code {
        1000 | 1001 => CloseAction::Normal,
        4004 | 4010 | 4011 | 4012 | 4013 | 4014 => CloseAction::Fatal,
        4007 | 4009 => CloseAction::Reconnect { resume: false },
        _ => CloseAction::Reconnect { resume: true },
    }
}

/// Returns true when the close code is fatal (reconnecting cannot help).
///
/// Reference: `docs/opcodes-and-status-codes.md` (Gateway Close Event Codes).
#[must_use]
pub const fn is_fatal(code: u16) -> bool {
    matches!(code, 4004 | 4010 | 4011 | 4012 | 4013 | 4014)
}

/// Builds the fatal [`GatewayError`] that `Client::start` returns for a close
/// code that must not be retried.
///
/// Reference: `docs/opcodes-and-status-codes.md` (Gateway Close Event Codes).
#[must_use]
pub fn fatal_error(code: u16, reason: &str) -> GatewayError {
    GatewayError::Closed {
        code,
        reason: reason.to_owned(),
        fatal: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documented_fatal_codes_are_fatal() {
        for code in [4004, 4010, 4011, 4012, 4013, 4014] {
            assert_eq!(classify(code), CloseAction::Fatal, "code {code}");
            assert!(is_fatal(code), "code {code}");
        }
    }

    #[test]
    fn client_initiated_clean_closes_are_normal() {
        assert_eq!(classify(1000), CloseAction::Normal);
        assert_eq!(classify(1001), CloseAction::Normal);
        assert!(!is_fatal(1000));
    }

    #[test]
    fn invalid_sequence_and_timed_out_session_reconnect_without_resume() {
        assert_eq!(classify(4007), CloseAction::Reconnect { resume: false });
        assert_eq!(classify(4009), CloseAction::Reconnect { resume: false });
    }

    #[test]
    fn other_documented_codes_reconnect_and_resume() {
        for code in [4000, 4001, 4002, 4003, 4005, 4008] {
            assert_eq!(
                classify(code),
                CloseAction::Reconnect { resume: true },
                "code {code}"
            );
            assert!(!is_fatal(code), "code {code}");
        }
    }

    #[test]
    fn unknown_codes_are_treated_as_reconnectable() {
        assert_eq!(classify(1005), CloseAction::Reconnect { resume: true });
        assert_eq!(classify(4999), CloseAction::Reconnect { resume: true });
    }

    #[test]
    fn every_documented_close_code_is_classified() {
        // docs/opcodes-and-status-codes.md (Gateway Close Event Codes): the
        // `Reconnect` column is `false` only for 4004 and 4010-4014. Codes 4007
        // and 4009 are documented as "Reconnect and start a new session", so
        // they reconnect without resuming.
        let documented = [
            (4000, CloseAction::Reconnect { resume: true }),
            (4001, CloseAction::Reconnect { resume: true }),
            (4002, CloseAction::Reconnect { resume: true }),
            (4003, CloseAction::Reconnect { resume: true }),
            (4004, CloseAction::Fatal),
            (4005, CloseAction::Reconnect { resume: true }),
            (4007, CloseAction::Reconnect { resume: false }),
            (4008, CloseAction::Reconnect { resume: true }),
            (4009, CloseAction::Reconnect { resume: false }),
            (4010, CloseAction::Fatal),
            (4011, CloseAction::Fatal),
            (4012, CloseAction::Fatal),
            (4013, CloseAction::Fatal),
            (4014, CloseAction::Fatal),
        ];

        for (code, expected) in documented {
            assert_eq!(classify(code), expected, "code {code}");

            let documented_reconnect = !matches!(expected, CloseAction::Fatal);
            assert_eq!(
                !is_fatal(code),
                documented_reconnect,
                "code {code} does not match the documented Reconnect column"
            );
        }
    }

    #[test]
    fn only_the_documented_codes_are_fatal() {
        for code in 1000..5000u16 {
            if matches!(code, 4004 | 4010 | 4011 | 4012 | 4013 | 4014) {
                assert!(is_fatal(code), "code {code} must be fatal");
            } else {
                assert!(!is_fatal(code), "code {code} must not be fatal");
            }
        }
    }

    #[test]
    fn the_zombie_close_code_is_not_a_clean_close() {
        assert_ne!(ZOMBIE_CLOSE_CODE, 1000);
        assert_ne!(ZOMBIE_CLOSE_CODE, 1001);
        assert_eq!(
            classify(ZOMBIE_CLOSE_CODE),
            CloseAction::Reconnect { resume: true }
        );
    }

    #[test]
    fn fatal_error_keeps_the_code_and_reason() {
        let error = fatal_error(4004, "Authentication failed.");

        match error {
            GatewayError::Closed {
                code,
                reason,
                fatal,
            } => {
                assert_eq!(code, 4004);
                assert_eq!(reason, "Authentication failed.");
                assert!(fatal);
            }
            other => panic!("expected a closed error, got {other:?}"),
        }
    }
}
