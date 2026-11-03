use discord::builder::command::CommandBuilder;
use discord::builder::embed::EmbedAuthorBuilder;
use discord::builder::embed::EmbedBuilder;
use discord::builder::interaction::ButtonBuilder;
use discord::builder::interaction::ReplyBuilder;
use discord::client::Context;
use discord::error::Error as DiscordError;
use discord::id::UserId;
use discord::model::application::CommandInteraction;
use discord::model::emoji::Emoji;

use tracing::info;
use tracing::warn;

use crate::config::Ids;
use crate::error::AppError;

/// Link target of the reply's button.
///
/// Taken from the package metadata, so it cannot drift from the repository the
/// bot was built from.
const REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// The description used when no creator user is configured.
const FALLBACK_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// Reads the configured ids out of the client's data map.
///
/// Start-up always publishes them, so a missing entry is a bug rather than a
/// user-facing condition.
///
/// # Errors
///
/// [`AppError::Denied`] when the data map has no [`Ids`] entry.
async fn configured_ids(ctx: &Context) -> Result<Ids, AppError> {
    ctx.data
        .read()
        .await
        .get::<Ids>()
        .copied()
        .ok_or(AppError::Denied(
            "The bot's configuration is not available.",
        ))
}

/// Turns the application-emoji lookup into the reply's optional icon.
///
/// The icon is decoration only: application emoji ids are per-application and
/// can go stale when the emoji is deleted or re-uploaded, and a failed lookup
/// used to fail the whole command with Discord error 10014 (Unknown emoji),
/// surfaced as HTTP 404. A lookup failure is therefore logged and dropped, so
/// `/info` still answers without the icon.
///
/// Reference: `docs/emoji.md` (Application-Owned Emoji, Get Application Emoji)
/// — the lookup is `GET /applications/{application.id}/emojis/{emoji.id}`, and
/// ids from another application are unknown to this one — and
/// `docs/opcodes-and-status-codes.md` (JSON Error Codes) — 10014 is
/// `Unknown emoji`.
fn best_effort_emoji(result: Result<Emoji, DiscordError>) -> Option<Emoji> {
    match result {
        Ok(emoji) => Some(emoji),
        Err(error) => {
            warn!(
                %error,
                "could not fetch the application emoji for /info; sending the reply without the icon"
            );

            None
        }
    }
}

/// Fetches the configured application emoji, when one is configured.
///
/// Returns `None` without a Discord round trip when `DISCORD_GITHUB_EMOJI_ID` is
/// unset; see [`best_effort_emoji`] for the failure behaviour.
async fn configured_emoji(ctx: &Context, ids: Ids) -> Option<Emoji> {
    let emoji_id = ids.github_emoji?;

    best_effort_emoji(ctx.get_application_emoji(emoji_id).await)
}

/// Builds the `/info` reply shown to the invoking user.
///
/// Split from [`run`] so the reply, including the optional creator mention and
/// button icon, can be serialised and asserted in tests without a live
/// [`Context`].
fn build_reply(
    bot_name: String,
    avatar_url: String,
    owner_id: Option<UserId>,
    emoji: Option<Emoji>,
) -> ReplyBuilder {
    let author = EmbedAuthorBuilder::new(bot_name).icon_url(avatar_url);

    let description = owner_id.map_or_else(
        || FALLBACK_DESCRIPTION.to_owned(),
        |owner_id| format!("Bot was created by: <@{owner_id}>"),
    );

    let embed = EmbedBuilder::new()
        .author(author)
        .color(0x00ff_ffff)
        .description(description);

    let mut button = ButtonBuilder::link(REPOSITORY_URL, "GitHub");
    if let Some(emoji) = emoji {
        button = button.emoji(emoji);
    }

    ReplyBuilder::new()
        .embed(embed.build())
        .button(button)
        .ephemeral()
}

pub(crate) async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), AppError> {
    // Read everything needed from the cached current user in one go.
    //
    // This used to re-fetch the bot's own account, which is a full Discord round
    // trip for data the cache already holds. The cache is populated from READY
    // (`docs/gateway-events.md`), so the name and avatar URL are available
    // locally. The cached value is an owned snapshot, so nothing is held across
    // the await below.
    let (bot_name, avatar_url): (String, String) = {
        let current_user = ctx.cache.current_user().ok_or(AppError::Denied(
            "The bot is still starting up, please try again in a moment.",
        ))?;

        (
            current_user.display_name().to_owned(),
            current_user.avatar_url(),
        )
    };

    let ids = configured_ids(ctx).await?;

    // Kept: an application emoji is not in the cache, so this one round trip is
    // the cost of showing the icon on the link button
    // (`docs/emoji.md`, Get Application Emoji: `GET
    // /applications/{application.id}/emojis/{emoji.id}`). It is best effort;
    // see [`configured_emoji`].
    let emoji = configured_emoji(ctx, ids).await;

    let response = build_reply(bot_name, avatar_url, ids.owner, emoji);

    info!("Received info command from {}.", interaction.user.username);
    interaction.respond(&ctx.http, response).await?;

    Ok(())
}

pub(crate) fn register() -> CommandBuilder {
    CommandBuilder::new("info", "Displays information about the bot.")
}

#[cfg(test)]
mod tests {
    use discord::error::HttpError;
    use discord::id::EmojiId;
    use discord::model::emoji::EmojiFlags;
    use serde_json::Value;
    use serde_json::json;

    use super::*;

    /// The creator user id used by the fixtures.
    fn owner_id() -> UserId {
        UserId::new(42)
    }

    /// The application emoji id used by the fixtures.
    fn emoji_id() -> EmojiId {
        EmojiId::new(999)
    }

    /// A fixture shaped like the documented application-emoji object
    /// (`docs/emoji.md`, Emoji Structure; `docs/emoji.md`, Get Application
    /// Emoji).
    fn github_emoji() -> Emoji {
        Emoji {
            id: Some(emoji_id()),
            name: Some("github".to_owned()),
            animated: false,
            roles: Vec::new(),
            user: None,
            flags: EmojiFlags {
                require_colons: true,
                managed: false,
                available: true,
            },
        }
    }

    /// Builds the reply fixture and serialises it for assertions.
    fn reply(owner_id: Option<UserId>, emoji: Option<Emoji>) -> Value {
        build_reply(
            "Example Bot".to_owned(),
            "https://cdn.discordapp.com/avatars/1/abc.png".to_owned(),
            owner_id,
            emoji,
        )
        .to_json()
    }

    /// The reply's single button, for concise assertions.
    fn button_json(reply: &Value) -> &Value {
        &reply["data"]["components"][0]["components"][0]
    }

    #[test]
    fn the_repository_url_is_configured() {
        // `REPOSITORY_URL` comes from `CARGO_PKG_REPOSITORY`, which is only set
        // when the package inherits `repository.workspace` in Cargo.toml. An
        // empty value would make Discord reject the link button, so guard
        // against the metadata silently going missing.
        assert!(
            REPOSITORY_URL.starts_with("https://"),
            "CARGO_PKG_REPOSITORY must be a non-empty https URL, got {REPOSITORY_URL:?}"
        );
    }

    #[test]
    fn reply_without_configuration_still_answers_with_the_link_button() {
        // Unconfigured optional values must not break the command: no creator
        // mention and no icon, but the normal embed and link button.
        let reply = reply(None, None);

        assert_eq!(reply["type"], json!(4));
        assert_eq!(reply["data"]["flags"], json!(64));
        assert_eq!(
            reply["data"]["embeds"][0]["description"],
            json!(FALLBACK_DESCRIPTION)
        );

        let button = button_json(&reply);

        assert_eq!(button["type"], json!(2));
        assert_eq!(button["style"], json!(5));
        assert_eq!(button["label"], json!("GitHub"));
        assert_eq!(button["url"], json!(REPOSITORY_URL));
        assert!(
            button.get("emoji").is_none(),
            "the reply must be valid without the icon: {reply}"
        );
    }

    #[test]
    fn a_configured_owner_is_mentioned_in_the_embed() {
        let reply = reply(Some(owner_id()), None);

        assert_eq!(
            reply["data"]["embeds"][0]["description"],
            json!("Bot was created by: <@42>")
        );
    }

    #[test]
    fn a_configured_emoji_is_put_on_the_link_button() {
        let reply = reply(Some(owner_id()), Some(github_emoji()));

        assert_eq!(
            button_json(&reply)["emoji"],
            json!({
                "id": emoji_id().to_string(),
                "name": "github"
            })
        );
    }

    #[test]
    fn a_404_unknown_emoji_lookup_becomes_a_missing_icon() {
        // The application-emoji lookup answering HTTP 404 with Discord code
        // 10014 (Unknown emoji, documented in
        // `docs/opcodes-and-status-codes.md`, JSON Error Codes) must become "no
        // icon", not a failed command.
        let error = DiscordError::Http(HttpError::Unsuccessful {
            status: 404,
            code: Some(10014),
            message: "Unknown emoji".to_owned(),
        });

        assert_eq!(best_effort_emoji(Err(error)), None);
        assert_eq!(best_effort_emoji(Ok(github_emoji())), Some(github_emoji()));
    }
}
