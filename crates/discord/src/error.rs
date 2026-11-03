//! The crate's error type.
//!
//! Errors are split by origin: an HTTP call failed, the gateway connection
//! failed, a payload was malformed, or local I/O failed. Each variant keeps the
//! information the caller needs to react — most importantly the HTTP status and
//! Discord JSON error code, which is how callers distinguish "the interaction
//! was already acknowledged" (code 40060) from a real fault.
//!
//! Reference: `docs/reference.md` (Error Messages) and
//! `docs/opcodes-and-status-codes.md` (HTTP Response Codes, JSON Error Codes).

use std::fmt;

/// Anything that can go wrong when talking to Discord.
#[derive(Debug)]
pub enum Error {
    /// A REST request failed.
    Http(HttpError),
    /// The gateway connection failed permanently.
    Gateway(GatewayError),
    /// A payload did not match the documented shape.
    Json(serde_json::Error),
    /// Local I/O failed.
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http(inner) => write!(f, "{inner}"),
            Self::Gateway(inner) => write!(f, "{inner}"),
            Self::Json(inner) => write!(f, "malformed Discord payload: {inner}"),
            Self::Io(inner) => write!(f, "I/O error: {inner}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(inner) => Some(inner),
            Self::Io(inner) => Some(inner),
            Self::Http(inner) => inner.source(),
            Self::Gateway(inner) => inner.source(),
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// A failed REST request.
#[derive(Debug)]
pub enum HttpError {
    /// Discord answered with a non-success status.
    Unsuccessful {
        /// HTTP status code.
        status: u16,
        /// Discord's JSON error code, when the response carried one.
        code: Option<i64>,
        /// Discord's human-readable message.
        message: String,
    },
    /// Discord answered with `429 Too Many Requests` and the retry budget was
    /// exhausted.
    RateLimited {
        /// Seconds Discord asked the client to wait.
        retry_after: f64,
        /// Whether the limit was the global per-bot limit.
        global: bool,
    },
    /// The request never produced a response (connection reset, DNS failure,
    /// timeout, ...).
    Transport(String),
}

impl HttpError {
    /// Returns the HTTP status code, when the request reached Discord.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Unsuccessful { status, .. } => Some(*status),
            Self::RateLimited { .. } => Some(429),
            Self::Transport(_) => None,
        }
    }

    /// Returns Discord's JSON error code, when one was present.
    ///
    /// For example, code 40060 means "interaction has already been
    /// acknowledged", which is how a command handler knows it must edit the
    /// deferred reply instead of acknowledging again.
    #[must_use]
    pub fn code(&self) -> Option<i64> {
        match self {
            Self::Unsuccessful { code, .. } => *code,
            _ => None,
        }
    }
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsuccessful {
                status,
                code,
                message,
            } => match code {
                Some(code) => write!(f, "HTTP {status} (Discord code {code}): {message}"),
                None => write!(f, "HTTP {status}: {message}"),
            },
            Self::RateLimited {
                retry_after,
                global,
            } => {
                if *global {
                    write!(
                        f,
                        "globally rate limited by Discord, retry after {retry_after:.2}s"
                    )
                } else {
                    write!(f, "rate limited by Discord, retry after {retry_after:.2}s")
                }
            }
            Self::Transport(message) => write!(f, "request failed: {message}"),
        }
    }
}

impl std::error::Error for HttpError {}

/// A failed gateway connection.
#[derive(Debug)]
pub enum GatewayError {
    /// The WebSocket closed with a close code, and the connection should not be
    /// retried (for example code 4004, authentication failed).
    Closed {
        /// The close code Discord sent.
        code: u16,
        /// A human-readable reason, when one was included.
        reason: String,
        /// Whether reconnecting could help. A fatal close means it cannot: the
        /// token, intents or API version must be fixed first.
        fatal: bool,
    },
    /// The payload stream violated the documented protocol.
    Protocol(String),
    /// The transport failed (TLS, TCP, WebSocket framing, ...).
    Transport(String),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed {
                code,
                reason,
                fatal,
            } => write!(
                f,
                "gateway closed with {} {}: {reason}",
                code,
                if *fatal { "(fatal)" } else { "(reconnectable)" }
            ),
            Self::Protocol(message) => write!(f, "gateway protocol error: {message}"),
            Self::Transport(message) => write!(f, "gateway transport error: {message}"),
        }
    }
}

impl std::error::Error for GatewayError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsuccessful_http_errors_expose_status_and_discord_code() {
        let error = Error::Http(HttpError::Unsuccessful {
            status: 400,
            code: Some(40060),
            message: "Interaction has already been acknowledged.".to_owned(),
        });

        let Error::Http(HttpError::Unsuccessful { status, code, .. }) = error else {
            panic!("expected an unsuccessful HTTP error");
        };

        assert_eq!(status, 400);
        assert_eq!(code, Some(40060));
    }

    #[test]
    fn display_includes_the_discord_code() {
        let error = HttpError::Unsuccessful {
            status: 403,
            code: Some(50013),
            message: "Missing Permissions".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "HTTP 403 (Discord code 50013): Missing Permissions"
        );
    }

    #[test]
    fn transport_errors_have_no_status() {
        assert_eq!(HttpError::Transport("timeout".to_owned()).status(), None);
    }

    #[test]
    fn fatal_gateway_close_is_marked_fatal() {
        let error = GatewayError::Closed {
            code: 4004,
            reason: "Authentication failed.".to_owned(),
            fatal: true,
        };

        assert!(error.to_string().contains("fatal"));
    }

    #[test]
    fn json_and_io_errors_are_sources() {
        use std::error::Error as _;

        let json: Error = serde_json::from_str::<u8>("not json")
            .expect_err("invalid")
            .into();
        assert!(json.source().is_some());

        let io: Error = std::io::Error::other("boom").into();
        assert!(io.source().is_some());
    }
}
