//! REST routes: request paths, per-route identities for the rate limiter, the
//! `X-Audit-Log-Reason` encoding and the JSON bodies of the endpoints the bot
//! uses.
//!
//! Paths come from the official Discord API reference. The `docs/<page>.md`
//! citations below point at that reference; its local copy is not vendored in
//! this template:
//!
//! * Create Message — `docs/message.md` (Create Message)
//! * Create Guild Ban / Remove Guild Member / Add Guild Member Role / Remove
//!   Guild Member Role — `docs/guild.md`
//! * Get Application Emoji — `docs/emoji.md` (Get Application Emoji)
//! * Application command registration —
//!   `docs/application-commands.md` (Endpoints)
//! * Interaction responses and followups — `docs/receiving-and-responding.md`
//!   (Endpoints)
//!
//! Everything here is pure construction with no I/O, so it is unit-tested
//! directly.

use crate::error::{Error, HttpError};
use crate::id::{
    ApplicationId, ChannelId, EmojiId, GuildId, InteractionId, MessageId, RoleId, UserId,
};

/// The HTTP method of a [`Route`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`
    Post,
    /// `PUT`
    Put,
    /// `PATCH`
    Patch,
    /// `DELETE`
    Delete,
}

impl Method {
    /// The upper-case method name as it appears on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// One documented REST endpoint: the HTTP method, the concrete path for this
/// call, the templated path used as the rate-limit route identity, and the
/// major (top-level) resource the route is limited on.
///
/// Reference: `docs/rate-limits.md` (Per-route rate limits) — limits are shared
/// by a set of endpoints (identified by `X-RateLimit-Bucket`) but computed
/// independently for the top-level `channel_id`, `guild_id` and webhook
/// resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    method: Method,
    path: String,
    key: String,
    major: Option<String>,
}

impl Route {
    /// Builds a route from its method, concrete path, templated path and major
    /// parameter.
    fn new(method: Method, path: String, template: &str, major: Option<String>) -> Self {
        let key = format!("{} {template}", method.as_str());

        Self {
            method,
            path,
            key,
            major,
        }
    }

    /// The HTTP method.
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// The concrete request path, beginning with `/` and excluding the base URL.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The templated route identity used to map routes onto rate-limit buckets,
    /// for example `POST /channels/{channel_id}/messages`.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The top-level resource this request is rate limited on, if any.
    ///
    /// Reference: `docs/rate-limits.md` (Per-route rate limits) — the identifier
    /// is the channel, guild or webhook id the route is scoped to.
    #[must_use]
    pub fn major(&self) -> Option<&str> {
        self.major.as_deref()
    }

    /// The absolute URL of this route below `base_url`.
    ///
    /// `base_url` is [`super::client::API_BASE`] in production.
    #[must_use]
    pub fn url(&self, base_url: &str) -> String {
        format!("{base_url}{}", self.path)
    }

    /// `POST /channels/{channel.id}/messages` — create a message.
    ///
    /// Reference: `docs/message.md` (Create Message).
    #[must_use]
    pub fn create_message(channel_id: ChannelId) -> Self {
        Self::new(
            Method::Post,
            format!("/channels/{channel_id}/messages"),
            "/channels/{channel_id}/messages",
            Some(format!("channel:{channel_id}")),
        )
    }

    /// `PUT /guilds/{guild.id}/bans/{user.id}` — create a guild ban.
    ///
    /// Reference: `docs/guild.md` (Create Guild Ban).
    #[must_use]
    pub fn create_guild_ban(guild_id: GuildId, user_id: UserId) -> Self {
        Self::new(
            Method::Put,
            format!("/guilds/{guild_id}/bans/{user_id}"),
            "/guilds/{guild_id}/bans/{user_id}",
            Some(format!("guild:{guild_id}")),
        )
    }

    /// `DELETE /guilds/{guild.id}/members/{user.id}` — remove a guild member.
    ///
    /// Reference: `docs/guild.md` (Remove Guild Member).
    #[must_use]
    pub fn remove_guild_member(guild_id: GuildId, user_id: UserId) -> Self {
        Self::new(
            Method::Delete,
            format!("/guilds/{guild_id}/members/{user_id}"),
            "/guilds/{guild_id}/members/{user_id}",
            Some(format!("guild:{guild_id}")),
        )
    }

    /// `PUT /guilds/{guild.id}/members/{user.id}/roles/{role.id}` — add a role
    /// to a guild member.
    ///
    /// The endpoint takes no JSON body and returns `204 No Content` on success;
    /// it supports the `X-Audit-Log-Reason` header.
    ///
    /// Reference: `docs/guild.md` (Add Guild Member Role).
    #[must_use]
    pub fn add_member_role(guild_id: GuildId, user_id: UserId, role_id: RoleId) -> Self {
        Self::new(
            Method::Put,
            format!("/guilds/{guild_id}/members/{user_id}/roles/{role_id}"),
            "/guilds/{guild_id}/members/{user_id}/roles/{role_id}",
            Some(format!("guild:{guild_id}")),
        )
    }

    /// `DELETE /guilds/{guild.id}/members/{user.id}/roles/{role.id}` — remove a
    /// role from a guild member.
    ///
    /// The endpoint takes no JSON body and returns `204 No Content` on success;
    /// it supports the `X-Audit-Log-Reason` header.
    ///
    /// Reference: `docs/guild.md` (Remove Guild Member Role).
    #[must_use]
    pub fn remove_member_role(guild_id: GuildId, user_id: UserId, role_id: RoleId) -> Self {
        Self::new(
            Method::Delete,
            format!("/guilds/{guild_id}/members/{user_id}/roles/{role_id}"),
            "/guilds/{guild_id}/members/{user_id}/roles/{role_id}",
            Some(format!("guild:{guild_id}")),
        )
    }

    /// `PUT /applications/{application.id}/guilds/{guild.id}/commands` —
    /// overwrite the guild's application commands.
    ///
    /// The route is scoped to one guild and `guild_id` is one of the top-level
    /// resources per-route limits are calculated on, so the guild is the
    /// rate-limit major parameter. The application id is *not* a documented
    /// top-level resource (`docs/rate-limits.md`, Per-route rate limits), so it
    /// does not key the bucket.
    ///
    /// Reference: `docs/application-commands.md` (Bulk Overwrite Guild
    /// Application Commands).
    #[must_use]
    pub fn set_guild_commands(application_id: ApplicationId, guild_id: GuildId) -> Self {
        Self::new(
            Method::Put,
            format!("/applications/{application_id}/guilds/{guild_id}/commands"),
            "/applications/{application_id}/guilds/{guild_id}/commands",
            Some(format!("guild:{guild_id}")),
        )
    }

    /// `PUT /applications/{application.id}/commands` — overwrite the global
    /// application commands.
    ///
    /// The application id is *not* a documented top-level resource
    /// (`docs/rate-limits.md`, Per-route rate limits), so the route carries no
    /// major parameter.
    ///
    /// Reference: `docs/application-commands.md` (Bulk Overwrite Global
    /// Application Commands).
    #[must_use]
    pub fn set_global_commands(application_id: ApplicationId) -> Self {
        Self::new(
            Method::Put,
            format!("/applications/{application_id}/commands"),
            "/applications/{application_id}/commands",
            None,
        )
    }

    /// `GET /applications/{application.id}/emojis/{emoji.id}` — fetch one of
    /// the application's own emoji.
    ///
    /// Emoji routes are documented to not follow the normal rate-limit
    /// conventions, and the path contains no documented top-level resource
    /// (channels, guilds, webhooks), so the route carries no major parameter.
    ///
    /// Reference: `docs/emoji.md` (Get Application Emoji) and
    /// `docs/rate-limits.md` (Per-route rate limits, emoji warning).
    #[must_use]
    pub fn get_application_emoji(application_id: ApplicationId, emoji_id: EmojiId) -> Self {
        Self::new(
            Method::Get,
            format!("/applications/{application_id}/emojis/{emoji_id}"),
            "/applications/{application_id}/emojis/{emoji_id}",
            None,
        )
    }

    /// `POST /interactions/{interaction.id}/{interaction.token}/callback` —
    /// respond to an interaction.
    ///
    /// Interaction endpoints are unauthenticated and exempt from the bot's
    /// global rate limit (`docs/rate-limits.md`, Global Rate Limit), so the
    /// route carries no major parameter.
    #[must_use]
    pub fn create_interaction_response(
        interaction_id: InteractionId,
        interaction_token: &str,
    ) -> Self {
        Self::new(
            Method::Post,
            format!("/interactions/{interaction_id}/{interaction_token}/callback"),
            "/interactions/{interaction_id}/{interaction_token}/callback",
            None,
        )
    }

    /// `PATCH /webhooks/{application.id}/{interaction.token}/messages/@original`
    /// — edit the interaction's original response.
    ///
    /// Interaction endpoints are unauthenticated and exempt from the bot's
    /// global rate limit (`docs/rate-limits.md`, Global Rate Limit).
    #[must_use]
    pub fn edit_interaction_response(
        application_id: ApplicationId,
        interaction_token: &str,
    ) -> Self {
        Self::new(
            Method::Patch,
            format!("/webhooks/{application_id}/{interaction_token}/messages/@original"),
            "/webhooks/{application_id}/{interaction_token}/messages/@original",
            None,
        )
    }

    /// `POST /interactions/{interaction.id}/{interaction.token}/callback` with
    /// `with_response=true` — respond and return the interaction callback
    /// response object.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Interaction
    /// Response, Query String Params, `with_response`). The rate-limit route
    /// identity is the same callback route (the query string does not change
    /// the endpoint).
    #[must_use]
    pub fn create_interaction_response_with_response(
        interaction_id: InteractionId,
        interaction_token: &str,
    ) -> Self {
        Self::new(
            Method::Post,
            format!(
                "/interactions/{interaction_id}/{interaction_token}/callback?with_response=true"
            ),
            "/interactions/{interaction_id}/{interaction_token}/callback",
            None,
        )
    }

    /// `GET /webhooks/{application.id}/{interaction.token}/messages/@original`
    /// — fetch the interaction's original response.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Get Original Interaction
    /// Response).
    #[must_use]
    pub fn get_interaction_response(
        application_id: ApplicationId,
        interaction_token: &str,
    ) -> Self {
        Self::new(
            Method::Get,
            format!("/webhooks/{application_id}/{interaction_token}/messages/@original"),
            "/webhooks/{application_id}/{interaction_token}/messages/@original",
            None,
        )
    }

    /// `DELETE /webhooks/{application.id}/{interaction.token}/messages/@original`
    /// — delete the interaction's original response.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Delete Original
    /// Interaction Response).
    #[must_use]
    pub fn delete_interaction_response(
        application_id: ApplicationId,
        interaction_token: &str,
    ) -> Self {
        Self::new(
            Method::Delete,
            format!("/webhooks/{application_id}/{interaction_token}/messages/@original"),
            "/webhooks/{application_id}/{interaction_token}/messages/@original",
            None,
        )
    }

    /// `POST /webhooks/{application.id}/{interaction.token}` — create a
    /// followup message; the endpoint always waits for the message, so a
    /// successful response carries the created message object.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Create Followup Message).
    #[must_use]
    pub fn create_followup_message(application_id: ApplicationId, interaction_token: &str) -> Self {
        Self::new(
            Method::Post,
            format!("/webhooks/{application_id}/{interaction_token}"),
            "/webhooks/{application_id}/{interaction_token}",
            None,
        )
    }

    /// `GET /webhooks/{application.id}/{interaction.token}/messages/{message.id}`
    /// — fetch a followup message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Get Followup Message).
    #[must_use]
    pub fn get_followup_message(
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: MessageId,
    ) -> Self {
        Self::new(
            Method::Get,
            format!("/webhooks/{application_id}/{interaction_token}/messages/{message_id}"),
            "/webhooks/{application_id}/{interaction_token}/messages/{message_id}",
            None,
        )
    }

    /// `PATCH /webhooks/{application.id}/{interaction.token}/messages/{message.id}`
    /// — edit a followup message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Edit Followup Message).
    #[must_use]
    pub fn edit_followup_message(
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: MessageId,
    ) -> Self {
        Self::new(
            Method::Patch,
            format!("/webhooks/{application_id}/{interaction_token}/messages/{message_id}"),
            "/webhooks/{application_id}/{interaction_token}/messages/{message_id}",
            None,
        )
    }

    /// `DELETE /webhooks/{application.id}/{interaction.token}/messages/{message.id}`
    /// — delete a followup message.
    ///
    /// Reference: `docs/receiving-and-responding.md` (Delete Followup Message).
    #[must_use]
    pub fn delete_followup_message(
        application_id: ApplicationId,
        interaction_token: &str,
        message_id: MessageId,
    ) -> Self {
        Self::new(
            Method::Delete,
            format!("/webhooks/{application_id}/{interaction_token}/messages/{message_id}"),
            "/webhooks/{application_id}/{interaction_token}/messages/{message_id}",
            None,
        )
    }
}

/// The maximum length of an `X-Audit-Log-Reason` value, in characters.
///
/// Reference: `docs/audit-log.md` (Audit Log Entry Object) — the header
/// supports 1-512 URL-encoded UTF-8 characters.
pub const MAX_AUDIT_LOG_REASON_CHARS: usize = 512;

/// Percent-encodes `value` as UTF-8 so it is safe in an HTTP header.
///
/// Everything except the RFC 3986 unreserved characters (`A-Z`, `a-z`, `0-9`,
/// `-`, `_`, `.`, `~`) is encoded as `%XX` with upper-case hex digits, so a
/// space becomes `%20` and non-ASCII text is encoded byte by byte.
///
/// Reference: `docs/audit-log.md` (Audit Log Entry Object) — the reason is
/// stored URL-encoded.
#[must_use]
pub fn percent_encode(value: &str) -> String {
    /// Whether `byte` may appear literally in a percent-encoded value.
    const fn is_unreserved(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')
    }

    /// Upper-case hexadecimal digits.
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if is_unreserved(byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }

    encoded
}

/// Encodes an audit-log reason for the `X-Audit-Log-Reason` header.
///
/// An empty reason produces `None` (no header is sent); a reason longer than
/// [`MAX_AUDIT_LOG_REASON_CHARS`] characters is truncated so Discord never
/// rejects the request with a `400`. The result is always ASCII.
///
/// Reference: `docs/audit-log.md` (Audit Log Entry Object).
#[must_use]
pub fn encode_audit_log_reason(reason: &str) -> Option<String> {
    if reason.is_empty() {
        return None;
    }

    let truncated: String = reason.chars().take(MAX_AUDIT_LOG_REASON_CHARS).collect();

    Some(percent_encode(&truncated))
}

/// Converts the deprecated `delete_message_days` ban argument into the
/// documented `delete_message_seconds` JSON field.
///
/// `days` is clamped to `0..=7` so the payload never leaves the documented
/// `0..=604800` second window.
///
/// Reference: `docs/guild.md` (Create Guild Ban, JSON Params) — "number of
/// seconds to delete messages for, between 0 and 604800".
#[must_use]
pub const fn ban_delete_message_seconds(delete_message_days: u8) -> u32 {
    let days = if delete_message_days > 7 {
        7
    } else {
        delete_message_days
    };

    days as u32 * 86_400
}

/// Builds the JSON body for `Create Guild Ban`.
///
/// Reference: `docs/guild.md` (Create Guild Ban, JSON Params).
#[must_use]
pub fn create_guild_ban_body(delete_message_days: u8) -> serde_json::Value {
    serde_json::json!({
        "delete_message_seconds": ban_delete_message_seconds(delete_message_days),
    })
}

/// Builds the documented error for a request whose application id is not known
/// yet.
///
/// The application id arrives with `READY`; commands cannot be registered
/// before the gateway has delivered it.
pub(crate) fn missing_application_id() -> Error {
    Error::Http(HttpError::Transport(
        "the application id is not known yet: wait for READY before registering commands"
            .to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_keeps_unreserved_characters() {
        let unreserved = "AZaz09-_.~";
        assert_eq!(percent_encode(unreserved), unreserved);
    }

    #[test]
    fn percent_encode_escapes_spaces_and_reserved_characters() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(
            percent_encode("a/b:c?d#e&f=g%h+i"),
            "a%2Fb%3Ac%3Fd%23e%26f%3Dg%25h%2Bi"
        );
    }

    #[test]
    fn percent_encode_encodes_utf8_bytes() {
        // "café ☕" as documented UTF-8 bytes.
        assert_eq!(percent_encode("café ☕"), "caf%C3%A9%20%E2%98%95");
    }

    #[test]
    fn empty_audit_log_reason_sends_no_header() {
        assert_eq!(encode_audit_log_reason(""), None);
    }

    #[test]
    fn audit_log_reason_is_percent_encoded() {
        assert_eq!(
            encode_audit_log_reason("banned: spam & abuse"),
            Some("banned%3A%20spam%20%26%20abuse".to_owned())
        );
    }

    #[test]
    fn audit_log_reason_is_truncated_to_the_documented_limit() {
        let reason = "é".repeat(MAX_AUDIT_LOG_REASON_CHARS + 100);
        let encoded = encode_audit_log_reason(&reason).expect("a non-empty reason");

        // Each "é" is two UTF-8 bytes, both percent-encoded.
        assert_eq!(encoded, "%C3%A9".repeat(MAX_AUDIT_LOG_REASON_CHARS));
    }

    #[test]
    fn ban_days_convert_to_the_documented_seconds_field() {
        assert_eq!(ban_delete_message_seconds(0), 0);
        assert_eq!(ban_delete_message_seconds(1), 86_400);
        assert_eq!(ban_delete_message_seconds(7), 604_800);
    }

    #[test]
    fn ban_days_are_clamped_to_the_documented_window() {
        assert_eq!(ban_delete_message_seconds(8), 604_800);
        assert_eq!(ban_delete_message_seconds(u8::MAX), 604_800);
    }

    #[test]
    fn create_guild_ban_body_uses_delete_message_seconds() {
        let body = create_guild_ban_body(3);

        assert_eq!(
            body,
            serde_json::json!({ "delete_message_seconds": 259_200 })
        );
        assert!(body.get("delete_message_days").is_none());
    }

    #[test]
    fn create_message_route_is_documented_and_templated() {
        let route = Route::create_message(ChannelId::new(1_111_111_111_111_111_111));

        assert_eq!(route.method(), Method::Post);
        assert_eq!(route.path(), "/channels/1111111111111111111/messages");
        assert_eq!(route.key(), "POST /channels/{channel_id}/messages");
        assert_eq!(route.major(), Some("channel:1111111111111111111"));
        assert_eq!(
            route.url("https://discord.com/api/v10"),
            "https://discord.com/api/v10/channels/1111111111111111111/messages"
        );
    }

    #[test]
    fn global_command_registration_route_has_no_major_parameter() {
        let route = Route::set_global_commands(ApplicationId::new(1_234_567_890_123_456_789));

        assert_eq!(route.method(), Method::Put);
        assert_eq!(route.path(), "/applications/1234567890123456789/commands");
        assert_eq!(route.key(), "PUT /applications/{application_id}/commands");
        assert_eq!(route.major(), None);
        assert_eq!(
            route.url("https://discord.com/api/v10"),
            "https://discord.com/api/v10/applications/1234567890123456789/commands"
        );
    }

    #[test]
    fn guild_routes_use_the_guild_as_major_parameter() {
        let guild = GuildId::new(1_000_000_000_000_000_001);
        let user = UserId::new(2_000_000_000_000_000_002);

        let ban = Route::create_guild_ban(guild, user);
        assert_eq!(ban.method(), Method::Put);
        assert_eq!(
            ban.path(),
            "/guilds/1000000000000000001/bans/2000000000000000002"
        );
        assert_eq!(ban.key(), "PUT /guilds/{guild_id}/bans/{user_id}");
        assert_eq!(ban.major(), Some("guild:1000000000000000001"));

        let kick = Route::remove_guild_member(guild, user);
        assert_eq!(kick.method(), Method::Delete);
        assert_eq!(
            kick.path(),
            "/guilds/1000000000000000001/members/2000000000000000002"
        );
        assert_eq!(kick.key(), "DELETE /guilds/{guild_id}/members/{user_id}");
        assert_eq!(kick.major(), Some("guild:1000000000000000001"));
    }

    #[test]
    fn member_role_routes_match_the_documented_paths() {
        // docs/guild.md (Add Guild Member Role, Remove Guild Member Role).
        let guild = GuildId::new(1_000_000_000_000_000_001);
        let user = UserId::new(2_000_000_000_000_000_002);
        let role = RoleId::new(3_000_000_000_000_000_003);

        let add = Route::add_member_role(guild, user, role);
        assert_eq!(add.method(), Method::Put);
        assert_eq!(
            add.path(),
            "/guilds/1000000000000000001/members/2000000000000000002/roles/3000000000000000003"
        );
        assert_eq!(
            add.key(),
            "PUT /guilds/{guild_id}/members/{user_id}/roles/{role_id}"
        );
        assert_eq!(add.major(), Some("guild:1000000000000000001"));

        let remove = Route::remove_member_role(guild, user, role);
        assert_eq!(remove.method(), Method::Delete);
        assert_eq!(
            remove.path(),
            "/guilds/1000000000000000001/members/2000000000000000002/roles/3000000000000000003"
        );
        assert_eq!(
            remove.key(),
            "DELETE /guilds/{guild_id}/members/{user_id}/roles/{role_id}"
        );
        assert_eq!(remove.major(), Some("guild:1000000000000000001"));

        assert_eq!(
            add.path(),
            remove.path(),
            "both endpoints address the same resource"
        );
    }

    #[test]
    fn application_routes_are_built_from_the_documented_paths() {
        let application = ApplicationId::new(42);
        let guild = GuildId::new(7);

        let commands = Route::set_guild_commands(application, guild);
        assert_eq!(commands.method(), Method::Put);
        assert_eq!(commands.path(), "/applications/42/guilds/7/commands");
        assert_eq!(
            commands.key(),
            "PUT /applications/{application_id}/guilds/{guild_id}/commands"
        );

        let emoji = Route::get_application_emoji(application, EmojiId::new(99));
        assert_eq!(emoji.method(), Method::Get);
        assert_eq!(emoji.path(), "/applications/42/emojis/99");
        assert_eq!(
            emoji.key(),
            "GET /applications/{application_id}/emojis/{emoji_id}"
        );
    }

    #[test]
    fn application_routes_use_only_documented_major_parameters() {
        // docs/rate-limits.md (Per-route rate limits): "Top-level resources are
        // currently limited to channels (`channel_id`), guilds (`guild_id`),
        // and webhooks (`webhook_id` or `webhook_id + webhook_token`)." The
        // application id is not one of them, so it must not key a bucket.
        let application = ApplicationId::new(42);

        // The guild-scoped command registration route is limited per guild.
        assert_eq!(
            Route::set_guild_commands(application, GuildId::new(7)).major(),
            Some("guild:7")
        );

        // Emoji routes do not follow normal rate-limit conventions
        // (docs/rate-limits.md, emoji warning) and the path has no documented
        // top-level resource.
        assert_eq!(
            Route::get_application_emoji(application, EmojiId::new(99)).major(),
            None
        );
    }

    #[test]
    fn interaction_routes_are_unauthenticated_and_not_globally_limited() {
        let interaction = InteractionId::new(555);
        let application = ApplicationId::new(42);

        let callback = Route::create_interaction_response(interaction, "tok.en");
        assert_eq!(callback.method(), Method::Post);
        assert_eq!(callback.path(), "/interactions/555/tok.en/callback");
        assert_eq!(
            callback.key(),
            "POST /interactions/{interaction_id}/{interaction_token}/callback"
        );
        assert_eq!(callback.major(), None);

        let edit = Route::edit_interaction_response(application, "tok.en");
        assert_eq!(edit.method(), Method::Patch);
        assert_eq!(edit.path(), "/webhooks/42/tok.en/messages/@original");
        assert_eq!(
            edit.key(),
            "PATCH /webhooks/{application_id}/{interaction_token}/messages/@original"
        );
        assert_eq!(edit.major(), None);
    }

    #[test]
    fn callback_with_response_keeps_the_callback_route_identity() {
        let route =
            Route::create_interaction_response_with_response(InteractionId::new(555), "tok.en");

        assert_eq!(route.method(), Method::Post);
        assert_eq!(
            route.path(),
            "/interactions/555/tok.en/callback?with_response=true"
        );
        assert_eq!(
            route.key(),
            "POST /interactions/{interaction_id}/{interaction_token}/callback"
        );
        assert_eq!(route.major(), None);
    }

    #[test]
    fn original_response_routes_are_documented() {
        let application = ApplicationId::new(42);

        let get = Route::get_interaction_response(application, "tok.en");
        assert_eq!(get.method(), Method::Get);
        assert_eq!(get.path(), "/webhooks/42/tok.en/messages/@original");
        assert_eq!(
            get.key(),
            "GET /webhooks/{application_id}/{interaction_token}/messages/@original"
        );
        assert_eq!(get.major(), None);

        let delete = Route::delete_interaction_response(application, "tok.en");
        assert_eq!(delete.method(), Method::Delete);
        assert_eq!(delete.path(), "/webhooks/42/tok.en/messages/@original");
        assert_eq!(
            delete.key(),
            "DELETE /webhooks/{application_id}/{interaction_token}/messages/@original"
        );
        assert_eq!(delete.major(), None);
    }

    #[test]
    fn followup_message_routes_are_documented() {
        let application = ApplicationId::new(42);
        let message = MessageId::new(99);

        let create = Route::create_followup_message(application, "tok.en");
        assert_eq!(create.method(), Method::Post);
        assert_eq!(create.path(), "/webhooks/42/tok.en");
        assert_eq!(
            create.key(),
            "POST /webhooks/{application_id}/{interaction_token}"
        );
        assert_eq!(create.major(), None);

        let get = Route::get_followup_message(application, "tok.en", message);
        assert_eq!(get.method(), Method::Get);
        assert_eq!(get.path(), "/webhooks/42/tok.en/messages/99");
        assert_eq!(
            get.key(),
            "GET /webhooks/{application_id}/{interaction_token}/messages/{message_id}"
        );

        let edit = Route::edit_followup_message(application, "tok.en", message);
        assert_eq!(edit.method(), Method::Patch);
        assert_eq!(edit.path(), "/webhooks/42/tok.en/messages/99");
        assert_eq!(
            edit.key(),
            "PATCH /webhooks/{application_id}/{interaction_token}/messages/{message_id}"
        );

        let delete = Route::delete_followup_message(application, "tok.en", message);
        assert_eq!(delete.method(), Method::Delete);
        assert_eq!(delete.path(), "/webhooks/42/tok.en/messages/99");
        assert_eq!(
            delete.key(),
            "DELETE /webhooks/{application_id}/{interaction_token}/messages/{message_id}"
        );
    }
}
