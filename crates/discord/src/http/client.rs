//! The REST client: authentication, rate-limit waits, retries and one method
//! per endpoint the bot uses.
//!
//! Every request follows the protocol in `docs/reference.md` (base URL,
//! `Authorization: Bot <token>`, `User-Agent`) and `docs/rate-limits.md`
//! (header-driven waits, `429` retry, bounded `5xx` retry). Non-success
//! responses are mapped onto [`HttpError`], the single error type the rest of
//! the crate uses.
//!
//! Endpoints that accept file uploads are documented to use a
//! `multipart/form-data` body (`docs/reference.md`, Uploading Files): create
//! message, the interaction callback, create followup, edit original and edit
//! followup. When the builder passed by the caller carries files, the request is
//! encoded as multipart with a `payload_json` part and `files[n]` parts; a
//! builder without files still sends a plain `application/json` body. The
//! multipart body is pre-encoded to owned bytes, so the retry/rate-limit loop
//! can rebuild an identical request without re-reading the files.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::rate_limit::{RateLimitHeaders, RateLimitInfo, RateLimiter};
use super::routes::{self, Method, Route};
use crate::builder::attachment::{AttachmentBuilder, MultipartBody, encode_multipart};
use crate::builder::command::CommandBuilder;
use crate::builder::interaction::{InteractionEditBuilder, InteractionResponse};
use crate::builder::message::MessageBuilder;
use crate::error::{Error, HttpError};
use crate::id::{
    ApplicationId, ChannelId, EmojiId, GuildId, InteractionId, MessageId, RoleId, UserId,
};
use crate::model::application::{Command, InteractionCallbackResponse};
use crate::model::emoji::Emoji;
use crate::model::message::Message;

/// Base URL of the v10 REST API.
///
/// Reference: `docs/reference.md` (Base URL, API Versioning).
pub const API_BASE: &str = "https://discord.com/api/v10";

/// The `User-Agent` header every request sends.
///
/// Reference: `docs/reference.md` (HTTP API, User Agent) — the documented
/// format is `DiscordBot ($url, $versionNumber)`.
pub const USER_AGENT: &str = concat!(
    "DiscordBot (https://github.com/gage-lodba/discord-bot-template, ",
    env!("CARGO_PKG_VERSION"),
    ")"
);

/// How long one HTTP attempt may take before it is treated as a transport
/// failure.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of attempts per request, including the first one.
const MAX_ATTEMPTS: u32 = 5;

/// Base delay for retrying a `5xx` response.
const RETRY_BACKOFF_BASE: Duration = Duration::from_millis(500);

/// Upper bound for the exponential `5xx` backoff.
const RETRY_BACKOFF_MAX: Duration = Duration::from_secs(8);

/// Longest raw (non-JSON) error body kept in an [`HttpError::Unsuccessful`]
/// message, so a large HTML error page cannot bloat a log line.
const MAX_ERROR_MESSAGE_CHARS: usize = 512;

/// Whether a request belongs to an endpoint family whose authentication and
/// rate-limit rules differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestKind {
    /// An ordinary bot request: `Authorization: Bot <token>` is sent and the
    /// global per-bot limit applies.
    Bot,
    /// An interaction callback or webhook edit: the interaction token in the
    /// path is the credential, no `Authorization` header is sent, and the
    /// request is exempt from the bot's global rate limit.
    ///
    /// Reference: `docs/rate-limits.md` (Global Rate Limit).
    Interaction,
}

/// The body of one request attempt.
///
/// Multipart bodies are kept as pre-encoded owned bytes so the retry loop can
/// rebuild the exact same request (and boundary) for every attempt without
/// re-reading the files.
#[derive(Debug)]
enum RequestPayload<'a> {
    /// A JSON body; reqwest serialises it and sets `Content-Type`.
    Json(&'a Value),
    /// A pre-encoded `multipart/form-data` body.
    Multipart(MultipartBody),
}

/// Chooses a plain JSON body or a multipart body for a payload and its files.
///
/// A request without files stays JSON, so every endpoint keeps the documented
/// `application/json` content type unless it actually uploads something
/// (`docs/reference.md`, Uploading Files).
fn payload_for<'a>(body: &'a Value, files: &[AttachmentBuilder]) -> RequestPayload<'a> {
    if files.is_empty() {
        RequestPayload::Json(body)
    } else {
        RequestPayload::Multipart(encode_multipart(body, files))
    }
}

/// A Discord REST client that authenticates with a bot token.
///
/// The client is cheap to clone (it shares one connection pool and one
/// rate-limit state); the bot can therefore keep one per [`Client`] and hand
/// clones to event handlers.
///
/// [`Client`]: crate::client::Client
#[derive(Clone)]
pub struct HttpClient {
    /// The bot token, sent as `Authorization: Bot <token>`.
    token: Arc<str>,
    /// Shared connection pool and TLS configuration.
    http: reqwest::Client,
    /// Per-route and global rate-limit state.
    rate_limiter: RateLimiter,
    /// The application id from `READY`; zero until it is known.
    application_id: Arc<AtomicU64>,
    /// API base URL, overridable in tests.
    base_url: Arc<str>,
}

impl HttpClient {
    /// Creates a client that authenticates with `token`.
    ///
    /// The token is sent as `Authorization: Bot <token>` and is never logged.
    /// The application id is unset until [`Self::set_application_id`] is called
    /// (the gateway does so when `READY` arrives).
    ///
    /// Reference: `docs/reference.md` (Authentication).
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        let http = match reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build() {
            Ok(http) => http,
            Err(error) => {
                tracing::warn!(%error, "could not build the configured HTTP client; using defaults");

                reqwest::Client::new()
            }
        };

        Self {
            token: Arc::from(token.into()),
            http,
            rate_limiter: RateLimiter::new(),
            application_id: Arc::new(AtomicU64::new(0)),
            base_url: Arc::from(API_BASE),
        }
    }

    /// Records the application id learned from `READY`, so convenience methods
    /// that need it can look it up.
    pub fn set_application_id(&self, id: ApplicationId) {
        self.application_id.store(id.get(), Ordering::Relaxed);
    }

    /// Returns the application id from `READY`, if it is known yet.
    #[must_use]
    pub fn application_id(&self) -> Option<ApplicationId> {
        match self.application_id.load(Ordering::Relaxed) {
            0 => None,
            value => Some(ApplicationId::new(value)),
        }
    }

    /// Returns the base URL this client talks to.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Creates a client for a test HTTP server instead of Discord.
    #[cfg(test)]
    fn with_base_url(token: impl Into<String>, base_url: impl Into<String>) -> Self {
        let mut client = Self::new(token);
        client.base_url = Arc::from(base_url.into());

        client
    }

    /// `POST /channels/{channel.id}/messages` — posts a message.
    ///
    /// Reference: `docs/message.md` (Create Message); request body built by
    /// [`MessageBuilder`]. A message with files is sent as
    /// `multipart/form-data` (`docs/reference.md`, Uploading Files).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn send_message(
        &self,
        channel_id: ChannelId,
        message: &MessageBuilder,
    ) -> Result<Message, Error> {
        let body = message.to_json();

        self.request_json_upload(
            &Route::create_message(channel_id),
            RequestKind::Bot,
            None,
            &body,
            message.files(),
        )
        .await
    }

    /// `PUT /guilds/{guild.id}/bans/{user.id}` — bans a member, optionally
    /// deleting their recent messages.
    ///
    /// `delete_message_days` is the deprecated 0-7 day argument; the request
    /// sends the documented `delete_message_seconds` field instead (clamped to
    /// at most 7 days).
    ///
    /// Reference: `docs/guild.md` (Create Guild Ban, JSON Params) and
    /// `docs/audit-log.md` (the `X-Audit-Log-Reason` header).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn ban_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        delete_message_days: u8,
        reason: Option<&str>,
    ) -> Result<(), Error> {
        let body = routes::create_guild_ban_body(delete_message_days);

        self.request_empty(
            &Route::create_guild_ban(guild_id, user_id),
            RequestKind::Bot,
            reason,
            Some(&body),
        )
        .await
    }

    /// `DELETE /guilds/{guild.id}/members/{user.id}` — kicks a member.
    ///
    /// Reference: `docs/guild.md` (Remove Guild Member) and
    /// `docs/audit-log.md` (the `X-Audit-Log-Reason` header).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn kick_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        reason: Option<&str>,
    ) -> Result<(), Error> {
        self.request_empty(
            &Route::remove_guild_member(guild_id, user_id),
            RequestKind::Bot,
            reason,
            None,
        )
        .await
    }

    /// `PUT /guilds/{guild.id}/members/{user.id}/roles/{role.id}` — adds a role
    /// to a guild member.
    ///
    /// The endpoint takes no request body and returns `204 No Content` on
    /// success. The caller needs the `MANAGE_ROLES` permission
    /// (`docs/guild.md`, Add Guild Member Role).
    ///
    /// Reference: `docs/guild.md` (Add Guild Member Role) and
    /// `docs/audit-log.md` (the `X-Audit-Log-Reason` header).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn add_member_role(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        role_id: RoleId,
        reason: Option<&str>,
    ) -> Result<(), Error> {
        self.request_empty(
            &Route::add_member_role(guild_id, user_id, role_id),
            RequestKind::Bot,
            reason,
            None,
        )
        .await
    }

    /// `DELETE /guilds/{guild.id}/members/{user.id}/roles/{role.id}` — removes a
    /// role from a guild member.
    ///
    /// The endpoint takes no request body and returns `204 No Content` on
    /// success. The caller needs the `MANAGE_ROLES` permission
    /// (`docs/guild.md`, Remove Guild Member Role).
    ///
    /// Reference: `docs/guild.md` (Remove Guild Member Role) and
    /// `docs/audit-log.md` (the `X-Audit-Log-Reason` header).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn remove_member_role(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        role_id: RoleId,
        reason: Option<&str>,
    ) -> Result<(), Error> {
        self.request_empty(
            &Route::remove_member_role(guild_id, user_id, role_id),
            RequestKind::Bot,
            reason,
            None,
        )
        .await
    }

    /// `PUT /applications/{application.id}/guilds/{guild.id}/commands` —
    /// overwrites the guild's slash commands and returns the registered ones.
    ///
    /// The application-commands page is not vendored; the wire shape is fixed
    /// by `docs/application-commands.md` (Endpoints).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not the documented array of
    /// command objects.
    pub async fn set_guild_commands(
        &self,
        application_id: ApplicationId,
        guild_id: GuildId,
        commands: &[CommandBuilder],
    ) -> Result<Vec<Command>, Error> {
        let body = Value::Array(commands.iter().map(CommandBuilder::to_json).collect());

        self.request_json(
            &Route::set_guild_commands(application_id, guild_id),
            RequestKind::Bot,
            None,
            Some(&body),
        )
        .await
    }

    /// `PUT /applications/{application.id}/commands` — overwrites the global
    /// slash commands and returns the registered ones.
    ///
    /// Global command changes are not reflected instantly by Discord; registering
    /// the same commands in a guild with [`Self::set_guild_commands`] shows up
    /// immediately, which is why a development setup prefers a configured guild.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not the documented array of
    /// command objects.
    pub async fn set_global_commands(
        &self,
        application_id: ApplicationId,
        commands: &[CommandBuilder],
    ) -> Result<Vec<Command>, Error> {
        let body = Value::Array(commands.iter().map(CommandBuilder::to_json).collect());

        self.request_json(
            &Route::set_global_commands(application_id),
            RequestKind::Bot,
            None,
            Some(&body),
        )
        .await
    }

    /// `GET /applications/{application.id}/emojis/{emoji.id}` — fetches one of
    /// the application's own emoji.
    ///
    /// Reference: `docs/emoji.md` (Get Application Emoji).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented emoji object.
    pub async fn get_application_emoji(
        &self,
        application_id: ApplicationId,
        emoji_id: EmojiId,
    ) -> Result<Emoji, Error> {
        self.request_json(
            &Route::get_application_emoji(application_id, emoji_id),
            RequestKind::Bot,
            None,
            None,
        )
        .await
    }

    /// `POST /interactions/{interaction.id}/{interaction.token}/callback` —
    /// acknowledges or replies to an interaction.
    ///
    /// The interaction token is the credential, so no `Authorization` header is
    /// sent and the request does not count against the bot's global limit.
    ///
    /// Reference: `docs/rate-limits.md` (Global Rate Limit); the callback shape
    /// is documented in `docs/receiving-and-responding.md` (Interaction Response Object).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn create_interaction_response(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        response: InteractionResponse,
    ) -> Result<(), Error> {
        let body = response.to_json();

        self.request_empty_upload(
            &Route::create_interaction_response(interaction_id, interaction_token),
            RequestKind::Interaction,
            None,
            &body,
            response.files(),
        )
        .await
    }

    /// `PATCH /webhooks/{application.id}/{interaction.token}/messages/@original`
    /// — edits the interaction's original reply.
    ///
    /// Like other interaction endpoints this is unauthenticated and exempt from
    /// the global rate limit (`docs/rate-limits.md`, Global Rate Limit).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_interaction_response(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        let files = edit.files();
        let body = edit.to_json();

        self.request_json_upload(
            &Route::edit_interaction_response(application_id, interaction_token),
            RequestKind::Interaction,
            None,
            &body,
            files,
        )
        .await
    }

    /// `POST /interactions/{interaction.id}/{interaction.token}/callback` with
    /// `with_response=true` — acknowledges or replies to an interaction and
    /// returns the documented interaction callback response object.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Interaction
    /// Response, Query String Params) — `with_response=true` makes the endpoint
    /// return `200` with the callback response instead of `204`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented interaction
    /// callback response object.
    pub async fn create_interaction_response_with_response(
        &self,
        interaction_id: InteractionId,
        interaction_token: &str,
        response: InteractionResponse,
    ) -> Result<InteractionCallbackResponse, Error> {
        let body = response.to_json();

        self.request_json_upload(
            &Route::create_interaction_response_with_response(interaction_id, interaction_token),
            RequestKind::Interaction,
            None,
            &body,
            response.files(),
        )
        .await
    }

    /// `GET /webhooks/{application.id}/{interaction.token}/messages/@original`
    /// — fetches the interaction's original response.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Get Original Interaction
    /// Response).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_interaction_response(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
    ) -> Result<Message, Error> {
        self.request_json(
            &Route::get_interaction_response(application_id, interaction_token),
            RequestKind::Interaction,
            None,
            None,
        )
        .await
    }

    /// `DELETE /webhooks/{application.id}/{interaction.token}/messages/@original`
    /// — deletes the interaction's original response.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Delete Original
    /// Interaction Response).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_interaction_response(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
    ) -> Result<(), Error> {
        self.request_empty(
            &Route::delete_interaction_response(application_id, interaction_token),
            RequestKind::Interaction,
            None,
            None,
        )
        .await
    }

    /// `POST /webhooks/{application.id}/{interaction.token}` — creates a
    /// followup message and returns it.
    ///
    /// Followup messages use `wait=true` semantics: the endpoint always
    /// responds with the created message object. Use
    /// [`crate::builder::message::MessageBuilder::ephemeral`] for an ephemeral
    /// followup (`docs/receiving-and-responding.md`, Create Followup Message).
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn create_followup_message(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message: &MessageBuilder,
    ) -> Result<Message, Error> {
        let body = message.to_json();

        self.request_json_upload(
            &Route::create_followup_message(application_id, interaction_token),
            RequestKind::Interaction,
            None,
            &body,
            message.files(),
        )
        .await
    }

    /// `GET /webhooks/{application.id}/{interaction.token}/messages/{message.id}`
    /// — fetches a followup message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Get Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn get_followup_message(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: MessageId,
    ) -> Result<Message, Error> {
        self.request_json(
            &Route::get_followup_message(application_id, interaction_token, message_id),
            RequestKind::Interaction,
            None,
            None,
        )
        .await
    }

    /// `PATCH /webhooks/{application.id}/{interaction.token}/messages/{message.id}`
    /// — edits a followup message and returns the updated message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Edit Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit), and
    /// [`Error::Json`] when the response body is not a documented message object.
    pub async fn edit_followup_message(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: MessageId,
        edit: InteractionEditBuilder,
    ) -> Result<Message, Error> {
        let files = edit.files();
        let body = edit.to_json();

        self.request_json_upload(
            &Route::edit_followup_message(application_id, interaction_token, message_id),
            RequestKind::Interaction,
            None,
            &body,
            files,
        )
        .await
    }

    /// `DELETE /webhooks/{application.id}/{interaction.token}/messages/{message.id}`
    /// — deletes a followup message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Delete Followup Message).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Http`] when the request cannot be sent or Discord answers
    /// with a non-success status (including an exhausted rate limit).
    pub async fn delete_followup_message(
        &self,
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: MessageId,
    ) -> Result<(), Error> {
        self.request_empty(
            &Route::delete_followup_message(application_id, interaction_token, message_id),
            RequestKind::Interaction,
            None,
            None,
        )
        .await
    }

    /// Executes a request and parses the JSON body of a successful response.
    async fn request_json<T: DeserializeOwned>(
        &self,
        route: &Route,
        kind: RequestKind,
        audit_log_reason: Option<&str>,
        body: Option<&Value>,
    ) -> Result<T, Error> {
        self.request_json_payload(
            route,
            kind,
            audit_log_reason,
            body.map(RequestPayload::Json),
        )
        .await
    }

    /// Executes a request whose JSON body may be accompanied by file uploads,
    /// and parses the JSON body of a successful response.
    ///
    /// Without files this is exactly [`Self::request_json`]; with files the
    /// payload is encoded as `multipart/form-data`
    /// (`docs/reference.md`, Uploading Files).
    async fn request_json_upload<T: DeserializeOwned>(
        &self,
        route: &Route,
        kind: RequestKind,
        audit_log_reason: Option<&str>,
        body: &Value,
        files: &[AttachmentBuilder],
    ) -> Result<T, Error> {
        self.request_json_payload(
            route,
            kind,
            audit_log_reason,
            Some(payload_for(body, files)),
        )
        .await
    }

    /// Executes a request whose successful response has no body.
    ///
    /// Reference: `docs/reference.md` (HTTP API) — `204 No Content` responses
    /// are successes with no value.
    async fn request_empty(
        &self,
        route: &Route,
        kind: RequestKind,
        audit_log_reason: Option<&str>,
        body: Option<&Value>,
    ) -> Result<(), Error> {
        self.execute(
            route,
            kind,
            audit_log_reason,
            body.map(RequestPayload::Json),
        )
        .await
        .map(|_| ())
    }

    /// Executes a bodyless-response request whose JSON body may be accompanied
    /// by file uploads.
    ///
    /// Without files this is exactly [`Self::request_empty`]; with files the
    /// payload is encoded as `multipart/form-data`.
    async fn request_empty_upload(
        &self,
        route: &Route,
        kind: RequestKind,
        audit_log_reason: Option<&str>,
        body: &Value,
        files: &[AttachmentBuilder],
    ) -> Result<(), Error> {
        let payload = payload_for(body, files);

        self.execute(route, kind, audit_log_reason, Some(payload))
            .await
            .map(|_| ())
    }

    /// Parses the JSON body of a request that was executed with an explicit
    /// payload.
    async fn request_json_payload<T: DeserializeOwned>(
        &self,
        route: &Route,
        kind: RequestKind,
        audit_log_reason: Option<&str>,
        payload: Option<RequestPayload<'_>>,
    ) -> Result<T, Error> {
        let bytes = self.execute(route, kind, audit_log_reason, payload).await?;

        serde_json::from_slice(&bytes).map_err(Error::Json)
    }

    /// Runs the request/retry loop and returns the body of a `2xx` response.
    ///
    /// The loop waits for a free rate-limit slot, sends the request, records the
    /// response's rate-limit headers, then retries `429` and `5xx` responses up
    /// to [`MAX_ATTEMPTS`] times (`docs/rate-limits.md`, Exceeding A Rate
    /// Limit). A multipart payload is pre-encoded owned bytes, so a retry sends
    /// the same body (and boundary) again.
    async fn execute(
        &self,
        route: &Route,
        kind: RequestKind,
        audit_log_reason: Option<&str>,
        payload: Option<RequestPayload<'_>>,
    ) -> Result<Vec<u8>, Error> {
        let mut attempt: u32 = 1;

        loop {
            // Interaction endpoints are not bound to the bot's global limit.
            if kind == RequestKind::Bot {
                self.rate_limiter.wait_for_global().await;
            }
            self.rate_limiter.wait_for_route(route).await;

            let request = self.build_request(route, kind, audit_log_reason, payload.as_ref());
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    self.rate_limiter.release(route);

                    return Err(transport_error(&error));
                }
            };

            let headers = response.headers().clone();
            let rate_headers = RateLimitHeaders::from_headers(&headers);
            self.rate_limiter.record_headers(route, &rate_headers);

            let status = response.status();
            let bytes = match response.bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(error) => {
                    self.rate_limiter.release(route);

                    return Err(transport_error(&error));
                }
            };

            if status.is_success() {
                return Ok(bytes);
            }

            if status == StatusCode::TOO_MANY_REQUESTS {
                let info = RateLimitInfo::from_response(&headers, &bytes);
                self.rate_limiter.record_rate_limit(route, &info);

                if attempt >= MAX_ATTEMPTS {
                    return Err(Error::Http(HttpError::RateLimited {
                        retry_after: info.retry_after.as_secs_f64(),
                        global: info.global,
                    }));
                }

                // The next iteration waits for the bucket/global limit the
                // response just recorded.
                attempt += 1;

                continue;
            }

            if status.is_server_error() {
                if attempt >= MAX_ATTEMPTS {
                    return Err(Error::Http(unsuccessful(status, &bytes)));
                }

                tokio::time::sleep(retry_backoff(attempt)).await;
                // The failed attempt consumed no quota.
                self.rate_limiter.release(route);
                attempt += 1;

                continue;
            }

            return Err(Error::Http(unsuccessful(status, &bytes)));
        }
    }

    /// Builds one HTTP request with the documented headers.
    ///
    /// Reference: `docs/reference.md` (Authentication, User Agent, Content
    /// Type, Uploading Files) and `docs/audit-log.md`
    /// (`X-Audit-Log-Reason`).
    fn build_request(
        &self,
        route: &Route,
        kind: RequestKind,
        audit_log_reason: Option<&str>,
        payload: Option<&RequestPayload<'_>>,
    ) -> reqwest::RequestBuilder {
        let method = match route.method() {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };

        let mut request = self
            .http
            .request(method, route.url(&self.base_url))
            .header("user-agent", USER_AGENT);

        if kind == RequestKind::Bot {
            request = request.header(AUTHORIZATION, format!("Bot {}", self.token));
        }

        if let Some(encoded) = audit_log_reason.and_then(routes::encode_audit_log_reason) {
            request = request.header("x-audit-log-reason", encoded);
        }

        match payload {
            None => request,
            Some(RequestPayload::Json(body)) => {
                // `.json` also sets `Content-Type: application/json`.
                request.json(*body)
            }
            Some(RequestPayload::Multipart(form)) => request
                .header(CONTENT_TYPE, form.content_type())
                // The owned body bytes are cloned per attempt, so retries work.
                .body(form.body().to_vec()),
        }
    }
}

impl fmt::Debug for HttpClient {
    /// Formats the client without exposing the bot token.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpClient")
            .field("base_url", &self.base_url)
            .field("application_id", &self.application_id())
            .finish_non_exhaustive()
    }
}

/// Maps a transport error onto [`HttpError::Transport`].
fn transport_error(error: &reqwest::Error) -> Error {
    Error::Http(HttpError::Transport(error.to_string()))
}

/// The exponential delay before retrying a `5xx` response: 500 ms, 1 s, 2 s,
/// 4 s, capped at [`RETRY_BACKOFF_MAX`].
fn retry_backoff(attempt: u32) -> Duration {
    let factor = 1_u32 << attempt.saturating_sub(1).min(4);

    RETRY_BACKOFF_BASE
        .saturating_mul(factor)
        .min(RETRY_BACKOFF_MAX)
}

/// The documented JSON error body: `{"code": i64, "message": string}`.
///
/// Reference: `docs/reference.md` (Error Messages).
#[derive(Debug, serde::Deserialize)]
struct ErrorBody {
    /// Discord's error code, such as `50035`.
    #[serde(default)]
    code: Option<i64>,
    /// Discord's human-readable message.
    #[serde(default)]
    message: Option<String>,
}

/// Converts a non-success response into an [`HttpError::Unsuccessful`].
///
/// A JSON body is parsed for `code` and `message`; any other body is kept as the
/// message (bounded to [`MAX_ERROR_MESSAGE_CHARS`]) so the caller still sees
/// what Discord said.
fn unsuccessful(status: StatusCode, body: &[u8]) -> HttpError {
    match serde_json::from_slice::<ErrorBody>(body) {
        Ok(ErrorBody {
            code,
            message: Some(message),
        }) => HttpError::Unsuccessful {
            status: status.as_u16(),
            code,
            message,
        },
        Ok(ErrorBody {
            code,
            message: None,
        }) => HttpError::Unsuccessful {
            status: status.as_u16(),
            code,
            message: fallback_message(status, body),
        },
        Err(_) => HttpError::Unsuccessful {
            status: status.as_u16(),
            code: None,
            message: fallback_message(status, body),
        },
    }
}

/// Extracts a message for a response whose body is missing or not Discord's
/// JSON error shape.
fn fallback_message(status: StatusCode, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let text = text.trim();

    if text.is_empty() {
        return status
            .canonical_reason()
            .unwrap_or("unknown HTTP error")
            .to_owned();
    }

    if text.chars().count() <= MAX_ERROR_MESSAGE_CHARS {
        return text.to_owned();
    }

    let mut truncated: String = text.chars().take(MAX_ERROR_MESSAGE_CHARS).collect();
    truncated.push('…');

    truncated
}

/// Returns a header value from a captured raw request, when present.
#[cfg(test)]
fn header_value<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        let (header, value) = line.split_once(':')?;

        header.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::builder::interaction::ReplyBuilder;
    use crate::id::GuildId;

    /// A `204` response with rate-limit headers.
    ///
    /// A `204` must not carry `Content-Length` (RFC 9110, 8.6).
    const EMPTY_204: &str = "HTTP/1.1 204 No Content\r\n\
        x-ratelimit-bucket: bucket\r\n\
        x-ratelimit-limit: 5\r\n\
        x-ratelimit-remaining: 5\r\n\
        x-ratelimit-reset-after: 1.5\r\n\
        connection: close\r\n\
        \r\n";

    /// Builds a complete HTTP/1.1 response with a correct `Content-Length`.
    fn http_response(status_line: &str, extra_headers: &[(&str, &str)], body: &str) -> String {
        let mut response = format!("HTTP/1.1 {status_line}\r\n");

        for (name, value) in extra_headers {
            let _ = write!(response, "{name}: {value}\r\n");
        }
        if !body.is_empty() {
            let _ = write!(response, "content-length: {}\r\n", body.len());
        }
        response.push_str("connection: close\r\n\r\n");
        response.push_str(body);

        response
    }

    /// Starts a tiny HTTP/1.1 server that plays `responses` in order and records
    /// every request it receives.
    ///
    /// The server stops after the last scripted response.
    async fn spawn_test_server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind a loopback listener");
        let address = listener.local_addr().expect("loopback listener address");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let requests = Arc::clone(&recorded);

        tokio::spawn(async move {
            for response in responses {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };

                let mut request = Vec::new();
                let mut chunk = [0_u8; 1024];

                // Read request headers, then as much of the body as the
                // `Content-Length` promises, so the server never closes the
                // socket while the client is still writing.
                let header_end = loop {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => break request.len(),
                        Ok(read) => {
                            request.extend_from_slice(&chunk[..read]);

                            if let Some(position) =
                                request.windows(4).position(|window| window == b"\r\n\r\n")
                            {
                                break position + 4;
                            }
                        }
                    }
                };

                let content_length = request[..header_end]
                    .split(|byte| *byte == b'\n')
                    .find_map(|line| {
                        let line = String::from_utf8_lossy(line);
                        let (name, value) = line.split_once(':')?;

                        name.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);

                while request.len() < header_end + content_length {
                    match socket.read(&mut chunk).await {
                        Ok(0) | Err(_) => break,
                        Ok(read) => request.extend_from_slice(&chunk[..read]),
                    }
                }

                requests
                    .lock()
                    .expect("request log is not poisoned")
                    .push(String::from_utf8_lossy(&request).into_owned());

                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        (format!("http://{address}"), recorded)
    }

    /// Runs a test future with a timeout so a broken HTTP exchange cannot hang
    /// the suite.
    async fn within_timeout<F: Future>(future: F) -> F::Output {
        tokio::time::timeout(Duration::from_secs(10), future)
            .await
            .expect("the HTTP test finished within the timeout")
    }

    #[test]
    fn user_agent_matches_the_documented_format() {
        // `DiscordBot ($url, $versionNumber)` with this repository's URL and the
        // crate version, which is derived from `CARGO_PKG_VERSION` so a version
        // bump cannot leave a stale header behind.
        assert_eq!(
            USER_AGENT,
            format!(
                "DiscordBot (https://github.com/gage-lodba/discord-bot-template, {})",
                env!("CARGO_PKG_VERSION")
            )
        );
        assert!(API_BASE.starts_with("https://discord.com/api/v"));
    }

    #[test]
    fn application_id_round_trips_through_the_client() {
        let client = HttpClient::new("token");
        assert_eq!(client.application_id(), None);

        client.set_application_id(ApplicationId::new(42));
        assert_eq!(client.application_id(), Some(ApplicationId::new(42)));
    }

    #[test]
    fn debug_does_not_leak_the_token() {
        let client = HttpClient::new("super-secret-token");
        let debug = format!("{client:?}");

        assert!(debug.contains("HttpClient"));
        assert!(!debug.contains("super-secret-token"));
    }

    #[test]
    fn server_error_retries_use_bounded_exponential_backoff() {
        assert_eq!(retry_backoff(1), Duration::from_millis(500));
        assert_eq!(retry_backoff(2), Duration::from_millis(1000));
        assert_eq!(retry_backoff(3), Duration::from_millis(2000));
        assert_eq!(retry_backoff(4), Duration::from_millis(4000));
        assert_eq!(retry_backoff(5), RETRY_BACKOFF_MAX);
        assert_eq!(retry_backoff(u32::MAX), RETRY_BACKOFF_MAX);
    }

    #[test]
    fn json_error_bodies_are_parsed() {
        let error = unsuccessful(
            StatusCode::BAD_REQUEST,
            br#"{"code":50035,"message":"Invalid Form Body"}"#,
        );

        match error {
            HttpError::Unsuccessful {
                status,
                code,
                message,
            } => {
                assert_eq!(status, 400);
                assert_eq!(code, Some(50035));
                assert_eq!(message, "Invalid Form Body");
            }
            other => panic!("expected Unsuccessful, got {other:?}"),
        }
    }

    #[test]
    fn json_error_body_without_a_code_keeps_the_message() {
        let error = unsuccessful(StatusCode::FORBIDDEN, br#"{"message":"Missing Access"}"#);

        match error {
            HttpError::Unsuccessful { code, message, .. } => {
                assert_eq!(code, None);
                assert_eq!(message, "Missing Access");
            }
            other => panic!("expected Unsuccessful, got {other:?}"),
        }
    }

    #[test]
    fn non_json_error_bodies_keep_the_raw_text() {
        let error = unsuccessful(StatusCode::BAD_GATEWAY, b"upstream <html> unavailable");

        match error {
            HttpError::Unsuccessful {
                status,
                code,
                message,
            } => {
                assert_eq!(status, 502);
                assert_eq!(code, None);
                assert!(message.contains("upstream <html> unavailable"));
            }
            other => panic!("expected Unsuccessful, got {other:?}"),
        }
    }

    #[test]
    fn empty_error_bodies_use_the_status_reason() {
        let error = unsuccessful(StatusCode::BAD_GATEWAY, b"");

        match error {
            HttpError::Unsuccessful { message, .. } => assert_eq!(message, "Bad Gateway"),
            other => panic!("expected Unsuccessful, got {other:?}"),
        }
    }

    #[test]
    fn huge_non_json_error_bodies_are_truncated() {
        let body = "x".repeat(MAX_ERROR_MESSAGE_CHARS * 4);
        let error = unsuccessful(StatusCode::INTERNAL_SERVER_ERROR, body.as_bytes());

        match error {
            HttpError::Unsuccessful { message, .. } => {
                assert_eq!(message.chars().count(), MAX_ERROR_MESSAGE_CHARS + 1);
                assert!(message.ends_with('…'));
            }
            other => panic!("expected Unsuccessful, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn bot_requests_send_authentication_user_agent_and_reason() {
        let (base_url, requests) = spawn_test_server(vec![EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        within_timeout(async {
            client
                .request_empty(
                    &Route::remove_guild_member(GuildId::new(1), UserId::new(2)),
                    RequestKind::Bot,
                    Some("clean up"),
                    None,
                )
                .await
        })
        .await
        .expect("the 204 is a success");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");
        let request_lower = request.to_ascii_lowercase();

        assert!(request_lower.starts_with("delete /guilds/1/members/2 http/1.1"));
        assert_eq!(
            header_value(request, "authorization"),
            Some("Bot test-token"),
            "missing bot authentication in {request}"
        );
        assert_eq!(header_value(request, "user-agent"), Some(USER_AGENT));
        assert_eq!(
            header_value(request, "x-audit-log-reason"),
            Some("clean%20up")
        );
    }

    #[tokio::test]
    async fn member_role_endpoints_use_the_documented_routes_and_reason_header() {
        // docs/guild.md (Add Guild Member Role, Remove Guild Member Role):
        // PUT/DELETE /guilds/{guild.id}/members/{user.id}/roles/{role.id},
        // 204 No Content, X-Audit-Log-Reason supported.
        let (base_url, requests) =
            spawn_test_server(vec![EMPTY_204.to_owned(), EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let guild = GuildId::new(1);
        let user = UserId::new(2);
        let role = RoleId::new(3);

        within_timeout(async {
            client
                .add_member_role(guild, user, role, Some("grant access"))
                .await
        })
        .await
        .expect("adding the role is a 204 success");

        within_timeout(async { client.remove_member_role(guild, user, role, None).await })
            .await
            .expect("removing the role is a 204 success");

        let requests = requests.lock().expect("request log is not poisoned");
        assert_eq!(requests.len(), 2);

        assert!(
            requests[0]
                .to_ascii_lowercase()
                .starts_with("put /guilds/1/members/2/roles/3 http/1.1"),
            "unexpected add request line: {}",
            requests[0]
        );
        assert_eq!(
            header_value(&requests[0], "x-audit-log-reason"),
            Some("grant%20access")
        );
        assert_eq!(
            header_value(&requests[0], "authorization"),
            Some("Bot test-token")
        );

        assert!(
            requests[1]
                .to_ascii_lowercase()
                .starts_with("delete /guilds/1/members/2/roles/3 http/1.1"),
            "unexpected remove request line: {}",
            requests[1]
        );
        assert_eq!(header_value(&requests[1], "x-audit-log-reason"), None);
    }

    #[tokio::test]
    async fn interaction_requests_are_unauthenticated() {
        let (base_url, requests) = spawn_test_server(vec![EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        within_timeout(async {
            client
                .request_empty(
                    &Route::create_interaction_response(InteractionId::new(1), "tok.en"),
                    RequestKind::Interaction,
                    None,
                    Some(&serde_json::json!({"type": 5})),
                )
                .await
        })
        .await
        .expect("the 204 is a success");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");
        let request_lower = request.to_ascii_lowercase();

        assert!(request_lower.starts_with("post /interactions/1/tok.en/callback http/1.1"));
        assert!(
            !request_lower.contains("authorization:"),
            "interaction endpoints must not send Authorization: {request}"
        );
        assert_eq!(header_value(request, "user-agent"), Some(USER_AGENT));
    }

    #[tokio::test]
    async fn a_429_is_retried_after_the_retry_after_window() {
        let body = r#"{"message":"You are being rate limited.","retry_after":0.01,"global":false}"#;
        let too_many_requests = http_response(
            "429 Too Many Requests",
            &[
                ("content-type", "application/json"),
                ("x-ratelimit-bucket", "bucket"),
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset-after", "0.01"),
                ("retry-after", "0.01"),
            ],
            body,
        );
        let (base_url, requests) =
            spawn_test_server(vec![too_many_requests, EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let guild = GuildId::new(1);
        let user = UserId::new(2);

        within_timeout(async {
            client
                .request_empty(
                    &Route::remove_guild_member(guild, user),
                    RequestKind::Bot,
                    None,
                    None,
                )
                .await
        })
        .await
        .expect("the retry succeeds");

        assert_eq!(
            requests.lock().expect("request log is not poisoned").len(),
            2,
            "the 429 must be retried exactly once"
        );
    }

    #[tokio::test]
    async fn a_5xx_is_retried_before_succeeding() {
        let server_error = http_response(
            "502 Bad Gateway",
            &[("content-type", "text/plain")],
            "upstream bad",
        );
        // The retry succeeds on the second attempt, so only one 500 ms backoff
        // is paid in the test.
        let (base_url, requests) =
            spawn_test_server(vec![server_error, EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let guild = GuildId::new(1);
        let user = UserId::new(2);

        within_timeout(async {
            client
                .request_empty(
                    &Route::remove_guild_member(guild, user),
                    RequestKind::Bot,
                    None,
                    None,
                )
                .await
        })
        .await
        .expect("the retry succeeds");

        assert_eq!(
            requests.lock().expect("request log is not poisoned").len(),
            2,
            "the 502 must be retried"
        );
    }

    #[tokio::test]
    async fn a_4xx_response_is_not_retried_and_keeps_the_discord_error() {
        let forbidden = http_response(
            "403 Forbidden",
            &[("content-type", "application/json")],
            r#"{"code":50013,"message":"Missing Permissions"}"#,
        );
        let (base_url, requests) = spawn_test_server(vec![forbidden]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        let result = within_timeout(async {
            client
                .request_empty(
                    &Route::remove_guild_member(GuildId::new(1), UserId::new(2)),
                    RequestKind::Bot,
                    None,
                    None,
                )
                .await
        })
        .await;

        match result {
            Err(Error::Http(HttpError::Unsuccessful {
                status,
                code,
                message,
            })) => {
                assert_eq!(status, 403);
                assert_eq!(code, Some(50013));
                assert_eq!(message, "Missing Permissions");
            }
            other => panic!("expected a non-retried 403, got {other:?}"),
        }

        assert_eq!(
            requests.lock().expect("request log is not poisoned").len(),
            1,
            "4xx responses must not be retried"
        );
    }

    #[tokio::test]
    async fn send_message_posts_the_builder_json_and_parses_the_message() {
        let message_json = serde_json::json!({
            "id": "334385199974967042",
            "channel_id": "290926798999357250",
            "author": {
                "id": "53908099506183680",
                "username": "Mason",
                "discriminator": "9999"
            },
            "content": "hello",
            "timestamp": "2017-07-11T17:27:07.299000+00:00",
            "embeds": [],
            "pinned": false
        })
        .to_string();
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &message_json,
        );
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        let message = within_timeout(async {
            client
                .send_message(
                    ChannelId::new(290_926_798_999_357_250),
                    &MessageBuilder::new().content("hello"),
                )
                .await
        })
        .await
        .expect("the documented message payload is parsed");

        assert_eq!(message.content, "hello");
        assert_eq!(message.channel_id, ChannelId::new(290_926_798_999_357_250));

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("post /channels/290926798999357250/messages http/1.1")
        );
        assert!(
            request.contains(r#""content":"hello""#),
            "the builder JSON must be the request body: {request}"
        );
        assert_eq!(
            header_value(request, "content-type"),
            Some("application/json")
        );
    }

    /// Returns the body of a captured HTTP request (everything after the first
    /// blank line).
    fn request_body(request: &str) -> &str {
        request.split_once("\r\n\r\n").map_or("", |(_, body)| body)
    }

    /// Extracts the boundary from a captured multipart `Content-Type` header.
    fn multipart_boundary(request: &str) -> String {
        let content_type = header_value(request, "content-type").expect("a Content-Type header");
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap_or_else(|| panic!("unexpected Content-Type: {content_type}"));

        boundary.to_owned()
    }

    #[tokio::test]
    async fn send_message_with_a_file_sends_the_documented_multipart_body() {
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("with a file"),
        );
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let message = MessageBuilder::new()
            .content("hello")
            .attachment(AttachmentBuilder::new("note.txt", b"file contents"));

        let sent =
            within_timeout(async { client.send_message(ChannelId::new(400), &message).await })
                .await
                .expect("the message response parses");
        assert_eq!(sent.content, "with a file");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("post /channels/400/messages http/1.1"),
            "unexpected request line: {request}"
        );
        assert_eq!(
            header_value(request, "authorization"),
            Some("Bot test-token")
        );
        assert_eq!(header_value(request, "user-agent"), Some(USER_AGENT));

        let boundary = multipart_boundary(request);
        let expected = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"payload_json\"\r\n\
             Content-Type: application/json\r\n\
             \r\n\
             {{\"attachments\":[{{\"filename\":\"note.txt\",\"id\":0}}],\"content\":\"hello\"}}\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"files[0]\"; filename=\"note.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             file contents\r\n\
             --{boundary}--\r\n"
        );

        assert_eq!(request_body(request), expected);
    }

    #[tokio::test]
    async fn interaction_callback_with_a_file_sends_the_documented_multipart_body() {
        let (base_url, requests) = spawn_test_server(vec![EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        within_timeout(async {
            client
                .create_interaction_response(
                    InteractionId::new(1),
                    "tok.en",
                    ReplyBuilder::new()
                        .content("Report attached")
                        .attachment(AttachmentBuilder::new("report.txt", b"report body"))
                        .into(),
                )
                .await
        })
        .await
        .expect("the callback is a 204 success");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");
        let request_lower = request.to_ascii_lowercase();

        assert!(
            request_lower.starts_with("post /interactions/1/tok.en/callback http/1.1"),
            "unexpected request line: {request}"
        );
        assert!(
            !request_lower.contains("authorization:"),
            "interaction endpoints must not send Authorization: {request}"
        );

        let boundary = multipart_boundary(request);
        let expected = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"payload_json\"\r\n\
             Content-Type: application/json\r\n\
             \r\n\
             {{\"data\":{{\"attachments\":[{{\"filename\":\"report.txt\",\"id\":0}}],\"content\":\"Report attached\"}},\"type\":4}}\r\n\
             --{boundary}\r\n\
             Content-Disposition: form-data; name=\"files[0]\"; filename=\"report.txt\"\r\n\
             Content-Type: text/plain\r\n\
             \r\n\
             report body\r\n\
             --{boundary}--\r\n"
        );

        assert_eq!(request_body(request), expected);
    }

    #[tokio::test]
    async fn a_multipart_request_retried_after_a_429_reuses_the_same_body() {
        let body = r#"{"message":"You are being rate limited.","retry_after":0.01,"global":false}"#;
        let too_many_requests = http_response(
            "429 Too Many Requests",
            &[
                ("content-type", "application/json"),
                ("x-ratelimit-bucket", "bucket"),
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset-after", "0.01"),
                ("retry-after", "0.01"),
            ],
            body,
        );
        let success = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("retried"),
        );
        let (base_url, requests) = spawn_test_server(vec![too_many_requests, success]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let message = MessageBuilder::new()
            .content("retry me")
            .attachment(AttachmentBuilder::new("a.png", [1_u8, 2, 3]));

        within_timeout(async { client.send_message(ChannelId::new(400), &message).await })
            .await
            .expect("the 429 retry succeeds");

        let requests = requests.lock().expect("request log is not poisoned");
        assert_eq!(requests.len(), 2, "the 429 must be retried once");
        assert_eq!(
            request_body(&requests[0]),
            request_body(&requests[1]),
            "the owned multipart bytes must be replayed unchanged on retry"
        );
        assert!(
            request_body(&requests[0]).contains(r#"name="files[0]"; filename="a.png""#),
            "the retried request is still multipart"
        );
        assert_eq!(
            multipart_boundary(&requests[0]),
            multipart_boundary(&requests[1]),
            "the boundary is stable across retries"
        );
    }

    #[tokio::test]
    async fn edit_original_response_with_a_file_uploads_it() {
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("edited"),
        );
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let edit = InteractionEditBuilder::new()
            .content("edited")
            .attachment(AttachmentBuilder::new("new.txt", b"new contents"));

        let edited = within_timeout(async {
            client
                .edit_interaction_response(ApplicationId::new(42), "tok.en", edit)
                .await
        })
        .await
        .expect("the edit response parses");
        assert_eq!(edited.content, "edited");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("patch /webhooks/42/tok.en/messages/@original http/1.1"),
            "unexpected request line: {request}"
        );
        let boundary = multipart_boundary(request);
        let body = request_body(request);

        assert!(
            body.contains(&format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\""
            )),
            "the edit body is the payload_json part: {body}"
        );
        assert!(body.contains(r#"name="files[0]"; filename="new.txt""#));
        assert!(body.contains(r#""attachments":[{"filename":"new.txt","id":0}]"#));
    }

    #[tokio::test]
    async fn create_followup_message_with_a_file_uploads_it() {
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("followup with file"),
        );
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let message = MessageBuilder::new()
            .content("followup")
            .attachments([AttachmentBuilder::new("a.txt", b"a")]);

        let sent = within_timeout(async {
            client
                .create_followup_message(ApplicationId::new(42), "tok.en", &message)
                .await
        })
        .await
        .expect("the followup message parses");
        assert_eq!(sent.content, "followup with file");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("post /webhooks/42/tok.en http/1.1"),
            "unexpected request line: {request}"
        );
        assert!(multipart_boundary(request).starts_with("----discord-"));
        assert!(request_body(request).contains(r#"name="files[0]"; filename="a.txt""#));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn ban_member_sends_delete_message_seconds_and_the_audit_reason() {
        let (base_url, requests) = spawn_test_server(vec![EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        within_timeout(async {
            client
                .ban_member(GuildId::new(1), UserId::new(2), 3, Some("spam"))
                .await
        })
        .await
        .expect("the 204 is a success");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("put /guilds/1/bans/2 http/1.1")
        );
        assert!(
            request.contains(r#""delete_message_seconds":259200"#),
            "Create Guild Ban must send delete_message_seconds: {request}"
        );
        assert!(
            !request.contains("delete_message_days"),
            "the deprecated field must not be sent: {request}"
        );
        assert_eq!(header_value(request, "x-audit-log-reason"), Some("spam"));
    }

    #[tokio::test]
    async fn set_guild_commands_puts_the_documented_route_and_command_array() {
        // docs/application-commands.md (Bulk Overwrite Guild Application
        // Commands): PUT /applications/{application.id}/guilds/{guild.id}/commands
        // takes a list of command objects and returns the registered array.
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            r#"[{"id":"900","application_id":"42","name":"ping","description":"Ping"}]"#,
        );
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let commands = [CommandBuilder::new("ping", "Ping")];

        let registered = within_timeout(async {
            client
                .set_guild_commands(ApplicationId::new(42), GuildId::new(7), &commands)
                .await
        })
        .await
        .expect("the registered command list parses");

        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].name, "ping");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("put /applications/42/guilds/7/commands http/1.1"),
            "unexpected request line: {request}"
        );
        assert_eq!(
            header_value(request, "authorization"),
            Some("Bot test-token")
        );
        assert!(
            request.contains(r#""name":"ping""#),
            "the command list is the request body: {request}"
        );
    }

    #[tokio::test]
    async fn set_global_commands_puts_the_documented_route_and_command_array() {
        // docs/application-commands.md (Bulk Overwrite Global Application
        // Commands): PUT /applications/{application.id}/commands takes a list of
        // command objects and returns the registered array.
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            r#"[{"id":"900","application_id":"42","name":"ping","description":"Ping"}]"#,
        );
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let commands = [CommandBuilder::new("ping", "Ping")];

        let registered = within_timeout(async {
            client
                .set_global_commands(ApplicationId::new(42), &commands)
                .await
        })
        .await
        .expect("the registered command list parses");

        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].name, "ping");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("put /applications/42/commands http/1.1"),
            "unexpected request line: {request}"
        );
        assert_eq!(
            header_value(request, "authorization"),
            Some("Bot test-token")
        );
        assert!(
            request.contains(r#""name":"ping""#),
            "the command list is the request body: {request}"
        );
    }

    #[tokio::test]
    async fn get_application_emoji_gets_the_documented_route() {
        // docs/emoji.md (Get Application Emoji):
        // GET /applications/{application.id}/emojis/{emoji.id} returns an
        // emoji object.
        let body = serde_json::json!({
            "id": "41771983429993937",
            "name": "LUL",
            "roles": [],
            "require_colons": true,
            "managed": false,
            "animated": false,
            "available": true
        })
        .to_string();
        let response = http_response("200 OK", &[("content-type", "application/json")], &body);
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        let emoji = within_timeout(async {
            client
                .get_application_emoji(ApplicationId::new(42), EmojiId::new(99))
                .await
        })
        .await
        .expect("the documented emoji payload parses");

        assert_eq!(emoji.id, Some(EmojiId::new(41_771_983_429_993_937)));
        assert_eq!(emoji.name.as_deref(), Some("LUL"));

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");

        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("get /applications/42/emojis/99 http/1.1"),
            "unexpected request line: {request}"
        );
        assert_eq!(
            header_value(request, "authorization"),
            Some("Bot test-token")
        );
    }

    #[test]
    fn build_request_sets_json_content_type_for_bodies() {
        let client = HttpClient::new("token");
        let body = serde_json::json!({"content": "hi"});
        let request = client
            .build_request(
                &Route::create_message(ChannelId::new(1)),
                RequestKind::Bot,
                None,
                Some(&RequestPayload::Json(&body)),
            )
            .build()
            .expect("the request is valid");

        assert_eq!(
            request
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bot token")
        );
    }

    #[test]
    fn build_request_omits_authorization_for_interactions() {
        let client = HttpClient::new("token");
        let request = client
            .build_request(
                &Route::create_interaction_response(InteractionId::new(1), "token"),
                RequestKind::Interaction,
                None,
                None,
            )
            .build()
            .expect("the request is valid");

        assert!(request.headers().get("authorization").is_none());
        assert_eq!(
            request
                .headers()
                .get("user-agent")
                .and_then(|value| value.to_str().ok()),
            Some(USER_AGENT)
        );
    }

    /// A documented message response body for the interaction webhook
    /// endpoints.
    fn interaction_message_body(content: &str) -> String {
        serde_json::json!({
            "id": "1000",
            "channel_id": "400",
            "author": {"id": "200", "username": "bot"},
            "timestamp": "2026-10-22T17:00:00.000000+00:00",
            "content": content,
            "pinned": false,
            "type": 20
        })
        .to_string()
    }

    #[tokio::test]
    async fn callback_with_response_returns_the_created_message() {
        let body = serde_json::json!({
            "interaction": {
                "id": "100",
                "type": 2,
                "response_message_id": "1000",
                "response_message_loading": false,
                "response_message_ephemeral": false
            },
            "resource": {
                "type": 4,
                "message": {
                    "id": "1000",
                    "channel_id": "400",
                    "author": {"id": "200", "username": "bot"},
                    "timestamp": "2026-10-22T17:00:00.000000+00:00",
                    "content": "Pong!",
                    "pinned": false,
                    "type": 20
                }
            }
        })
        .to_string();
        let response = http_response("200 OK", &[("content-type", "application/json")], &body);
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        let callback = within_timeout(async {
            client
                .create_interaction_response_with_response(
                    InteractionId::new(100),
                    "tok.en",
                    InteractionResponse::Pong,
                )
                .await
        })
        .await
        .expect("the callback response parses");

        assert_eq!(callback.interaction.id, InteractionId::new(100));
        assert_eq!(
            callback
                .created_message()
                .map(|message| message.content.as_str()),
            Some("Pong!")
        );

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");
        let request_lower = request.to_ascii_lowercase();

        assert!(
            request_lower.starts_with("post /interactions/100/tok.en/callback?with_response=true")
        );
        assert!(
            !request_lower.contains("authorization:"),
            "interaction endpoints must not send Authorization: {request}"
        );
        assert!(request.contains(r#""type":1"#));
    }

    #[tokio::test]
    async fn original_response_can_be_fetched_and_deleted() {
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("original"),
        );
        let (base_url, requests) = spawn_test_server(vec![response, EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        let message = within_timeout(async {
            client
                .get_interaction_response(ApplicationId::new(42), "tok.en")
                .await
        })
        .await
        .expect("the original response parses");
        assert_eq!(message.content, "original");

        within_timeout(async {
            client
                .delete_interaction_response(ApplicationId::new(42), "tok.en")
                .await
        })
        .await
        .expect("the delete succeeds");

        let requests = requests.lock().expect("request log is not poisoned");
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .starts_with("get /webhooks/42/tok.en/messages/@original http/1.1")
        );
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .starts_with("delete /webhooks/42/tok.en/messages/@original http/1.1")
        );
        assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn followup_messages_can_be_created() {
        let response = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("followup"),
        );
        let (base_url, requests) = spawn_test_server(vec![response]).await;
        let client = HttpClient::with_base_url("test-token", base_url);

        let message = within_timeout(async {
            client
                .create_followup_message(
                    ApplicationId::new(42),
                    "tok.en",
                    &MessageBuilder::new().content("followup").ephemeral(),
                )
                .await
        })
        .await
        .expect("the followup message parses");

        assert_eq!(message.content, "followup");

        let requests = requests.lock().expect("request log is not poisoned");
        let request = requests.first().expect("one request was made");
        assert!(
            request
                .to_ascii_lowercase()
                .starts_with("post /webhooks/42/tok.en http/1.1"),
            "unexpected request line: {request}"
        );
        assert!(request.contains(r#""content":"followup""#));
        assert!(
            request.contains(r#""flags":64"#),
            "the ephemeral flag must be sent: {request}"
        );
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
    }

    #[tokio::test]
    async fn followup_messages_can_be_fetched_edited_and_deleted() {
        let get = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("followup"),
        );
        let edit = http_response(
            "200 OK",
            &[("content-type", "application/json")],
            &interaction_message_body("edited"),
        );
        let (base_url, requests) = spawn_test_server(vec![get, edit, EMPTY_204.to_owned()]).await;
        let client = HttpClient::with_base_url("test-token", base_url);
        let message_id = MessageId::new(1000);

        let message = within_timeout(async {
            client
                .get_followup_message(ApplicationId::new(42), "tok.en", message_id)
                .await
        })
        .await
        .expect("the followup message parses");
        assert_eq!(message.content, "followup");

        let edited = within_timeout(async {
            client
                .edit_followup_message(
                    ApplicationId::new(42),
                    "tok.en",
                    message_id,
                    InteractionEditBuilder::new().content("edited"),
                )
                .await
        })
        .await
        .expect("the edited followup message parses");
        assert_eq!(edited.content, "edited");

        within_timeout(async {
            client
                .delete_followup_message(ApplicationId::new(42), "tok.en", message_id)
                .await
        })
        .await
        .expect("the delete succeeds");

        let requests = requests.lock().expect("request log is not poisoned");
        assert_eq!(requests.len(), 3);
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .starts_with("get /webhooks/42/tok.en/messages/1000 http/1.1")
        );
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .starts_with("patch /webhooks/42/tok.en/messages/1000 http/1.1")
        );
        assert!(requests[1].contains(r#""content":"edited""#));
        assert!(
            requests[2]
                .to_ascii_lowercase()
                .starts_with("delete /webhooks/42/tok.en/messages/1000 http/1.1")
        );
    }
}
