use std::env;
use std::fmt;

use discord::id::ChannelId;
use discord::id::EmojiId;
use discord::id::GuildId;
use discord::id::UserId;

/// Name of the environment variable holding the Discord bot token.
pub(crate) const TOKEN_ENV_VAR: &str = "DISCORD_TOKEN";
/// Guild the bot serves and registers its slash commands in.
pub(crate) const GUILD_ID_ENV_VAR: &str = "DISCORD_GUILD_ID";
/// Channel where join/leave/kick/ban announcements are posted.
pub(crate) const LOG_CHANNEL_ID_ENV_VAR: &str = "DISCORD_LOG_CHANNEL_ID";
/// Optional user mentioned as the bot's creator in `/info`.
pub(crate) const OWNER_ID_ENV_VAR: &str = "DISCORD_OWNER_ID";
/// Optional application emoji used as the `/info` link button's icon.
pub(crate) const GITHUB_EMOJI_ID_ENV_VAR: &str = "DISCORD_GITHUB_EMOJI_ID";
/// Optional invite URL shown in the `/dashboard` invite embed.
pub(crate) const INVITE_URL_ENV_VAR: &str = "DISCORD_INVITE_URL";

/// Discord ids the bot operates on.
///
/// Held separately from [`Config`] because the event handler needs these but has
/// no business holding the token.
///
/// Also published into the client's data map so command handlers can read the
/// configured ids from [`discord::Context::data`] without every `run()` taking an
/// extra parameter.
///
/// The two optional ids only shape `/info`: without `owner` the embed omits the
/// creator mention, and without `github_emoji` the link button has no icon.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ids {
    /// Guild the bot serves.
    pub(crate) guild: GuildId,
    /// Channel moderation events are announced in.
    pub(crate) log_channel: ChannelId,
    /// User credited as the bot's creator in `/info`, when configured.
    pub(crate) owner: Option<UserId>,
    /// Application emoji used as the `/info` link button icon, when configured.
    pub(crate) github_emoji: Option<EmojiId>,
}

/// Makes [`Ids`] storable in the client's type map.
impl discord::DataKey for Ids {
    type Value = Ids;
}

/// Free-form deployment content that is not a Discord id.
///
/// Published into the client's data map next to [`Ids`], so the command layer
/// reads it without a global.
#[derive(Clone, Debug)]
pub(crate) struct Dashboard {
    /// Invite URL shown in the `/dashboard` invite embed, when configured.
    pub(crate) invite_url: Option<String>,
}

/// Makes [`Dashboard`] storable in the client's type map.
impl discord::DataKey for Dashboard {
    type Value = Dashboard;
}

/// Runtime configuration.
pub(crate) struct Config {
    /// The bot token. Never logged: see the [`fmt::Debug`] implementation.
    pub(crate) token: String,
    /// Discord ids the bot operates on.
    pub(crate) ids: Ids,
    /// Dashboard content configured for this deployment.
    pub(crate) dashboard: Dashboard,
}

/// Redacts the token.
///
/// A derived `Debug` would print the token in full, and `Config` is exactly the
/// kind of value that ends up in a log line or a panic message.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field(
                "token",
                &format_args!("<redacted, {} chars>", self.token.len()),
            )
            .field("ids", &self.ids)
            .field("dashboard", &self.dashboard)
            .finish()
    }
}

impl Config {
    /// Builds the configuration from the environment.
    ///
    /// # Errors
    ///
    /// Returns an error naming the offending variable when a required variable is
    /// unset or empty, when an id is not a non-zero snowflake, or when the invite
    /// URL is not an `http`/`https` URL.
    pub(crate) fn from_env() -> Result<Self, String> {
        Self::from_lookup(|name| env::var(name).ok())
    }

    /// Builds the configuration from `lookup`.
    ///
    /// Split from [`Self::from_env`] so the validation can be tested without
    /// mutating the process environment, which is not thread safe.
    ///
    /// # Errors
    ///
    /// See [`Self::from_env`]. Error messages never contain the token value.
    pub(crate) fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let token = lookup(TOKEN_ENV_VAR)
            .ok_or_else(|| {
                format!(
                    "{TOKEN_ENV_VAR} is not set. Export it from your shell before start-up; \
                     do not hard-code it in the source."
                )
            })?
            .trim()
            .to_owned();

        if token.is_empty() {
            return Err(format!("{TOKEN_ENV_VAR} is set but empty"));
        }

        // The deployment ids have no baked-in defaults: this repository is
        // public, so it must not carry another server's snowflakes. A missing
        // required variable fails at start-up instead.
        let ids = Ids {
            guild: GuildId::new(required_id(&lookup, GUILD_ID_ENV_VAR)?),
            log_channel: ChannelId::new(required_id(&lookup, LOG_CHANNEL_ID_ENV_VAR)?),
            owner: optional_id(&lookup, OWNER_ID_ENV_VAR)?.map(UserId::new),
            github_emoji: optional_id(&lookup, GITHUB_EMOJI_ID_ENV_VAR)?.map(EmojiId::new),
        };

        let dashboard = Dashboard {
            invite_url: optional_invite_url(&lookup)?,
        };

        Ok(Self {
            token,
            ids,
            dashboard,
        })
    }
}

/// Reads a required snowflake.
///
/// # Errors
///
/// Returns a message naming the variable when it is unset or blank, when it is
/// not numeric, or when it is zero.
fn required_id(lookup: &impl Fn(&str) -> Option<String>, name: &str) -> Result<u64, String> {
    let raw = lookup(name).ok_or_else(|| format!("{name} is not set"))?;

    parse_id(name, raw.trim())
}

/// Reads an optional snowflake, treating an unset or blank variable as absent.
///
/// # Errors
///
/// Returns a message naming the variable when it is set to something that is not
/// a non-zero snowflake.
fn optional_id(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &str,
) -> Result<Option<u64>, String> {
    let Some(raw) = lookup(name) else {
        return Ok(None);
    };

    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Ok(None);
    }

    parse_id(name, trimmed).map(Some)
}

/// Parses one snowflake value, rejecting zero and non-numeric input.
///
/// `Id::new` panics on zero, so zero is rejected here rather than allowed to
/// abort the process during start-up.
///
/// # Errors
///
/// Returns a message naming `name` and quoting its value when `raw` cannot be a
/// Discord id.
fn parse_id(name: &str, raw: &str) -> Result<u64, String> {
    let value: u64 = raw
        .parse()
        .map_err(|_| format!("{name} must be a numeric Discord id, got {raw:?}"))?;

    if value == 0 {
        return Err(format!("{name} must not be zero"));
    }

    Ok(value)
}

/// Reads the optional invite URL shown by `/dashboard`.
///
/// # Errors
///
/// Returns a message naming the variable when it is set to something that is not
/// an `http`/`https` URL.
fn optional_invite_url(lookup: &impl Fn(&str) -> Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = lookup(INVITE_URL_ENV_VAR) else {
        return Ok(None);
    };

    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Ok(None);
    }

    let lower = trimmed.to_ascii_lowercase();
    let scheme_len = if lower.starts_with("https://") {
        "https://".len()
    } else if lower.starts_with("http://") {
        "http://".len()
    } else {
        0
    };

    // A scheme with no host is not a usable URL either.
    if scheme_len == 0 || trimmed.len() == scheme_len {
        return Err(format!(
            "{INVITE_URL_ENV_VAR} must be an http(s) URL, got {trimmed:?}"
        ));
    }

    Ok(Some(trimmed.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lookup that never finds anything.
    fn unset(_: &str) -> Option<String> {
        None
    }

    /// A lookup with a token and both required ids set.
    fn configured(name: &str) -> Option<String> {
        match name {
            TOKEN_ENV_VAR => Some("abc.def.ghi".to_owned()),
            GUILD_ID_ENV_VAR => Some("101".to_owned()),
            LOG_CHANNEL_ID_ENV_VAR => Some("202".to_owned()),
            _ => None,
        }
    }

    #[test]
    fn missing_token_is_reported_without_including_a_value() {
        let err = Config::from_lookup(unset).expect_err("no token present");

        assert!(err.contains(TOKEN_ENV_VAR));
        assert!(err.contains("not set"));
    }

    #[test]
    fn blank_token_is_rejected() {
        let err = Config::from_lookup(|_| Some("   \t ".to_owned())).expect_err("blank rejected");

        assert!(err.contains("empty"));
    }

    #[test]
    fn token_is_trimmed() {
        let config = Config::from_lookup(|name| {
            if name == TOKEN_ENV_VAR {
                Some("  abc.def.ghi  ".to_owned())
            } else {
                configured(name)
            }
        })
        .expect("token accepted");

        assert_eq!(config.token, "abc.def.ghi");
    }

    #[test]
    fn required_ids_are_parsed() {
        let config = Config::from_lookup(configured).expect("configuration accepted");

        assert_eq!(config.ids.guild, GuildId::new(101));
        assert_eq!(config.ids.log_channel, ChannelId::new(202));
    }

    #[test]
    fn a_missing_guild_id_is_rejected() {
        let err = Config::from_lookup(|name| match name {
            TOKEN_ENV_VAR => Some("abc.def.ghi".to_owned()),
            LOG_CHANNEL_ID_ENV_VAR => Some("202".to_owned()),
            _ => None,
        })
        .expect_err("guild id required");

        assert!(err.contains(GUILD_ID_ENV_VAR));
        assert!(err.contains("not set"));
    }

    #[test]
    fn a_missing_log_channel_is_rejected() {
        let err = Config::from_lookup(|name| match name {
            TOKEN_ENV_VAR => Some("abc.def.ghi".to_owned()),
            GUILD_ID_ENV_VAR => Some("101".to_owned()),
            _ => None,
        })
        .expect_err("log channel required");

        assert!(err.contains(LOG_CHANNEL_ID_ENV_VAR));
        assert!(err.contains("not set"));
    }

    #[test]
    fn non_numeric_id_is_rejected_with_the_variable_name() {
        let err = Config::from_lookup(|name| match name {
            TOKEN_ENV_VAR => Some("abc.def.ghi".to_owned()),
            GUILD_ID_ENV_VAR => Some("not-a-number".to_owned()),
            LOG_CHANNEL_ID_ENV_VAR => Some("202".to_owned()),
            _ => None,
        })
        .expect_err("non-numeric id rejected");

        assert!(err.contains(GUILD_ID_ENV_VAR));
        assert!(err.contains("numeric"));
    }

    #[test]
    fn zero_id_is_rejected_rather_than_panicking() {
        // `GuildId::new(0)` panics; configuration must fail cleanly instead.
        let err = Config::from_lookup(|name| match name {
            TOKEN_ENV_VAR => Some("abc.def.ghi".to_owned()),
            GUILD_ID_ENV_VAR => Some("101".to_owned()),
            LOG_CHANNEL_ID_ENV_VAR => Some("0".to_owned()),
            _ => None,
        })
        .expect_err("zero id rejected");

        assert!(err.contains(LOG_CHANNEL_ID_ENV_VAR));
        assert!(err.contains("zero"));
    }

    #[test]
    fn optional_ids_are_absent_by_default() {
        let config = Config::from_lookup(configured).expect("configuration accepted");

        assert_eq!(config.ids.owner, None);
        assert_eq!(config.ids.github_emoji, None);
        assert_eq!(config.dashboard.invite_url, None);
    }

    #[test]
    fn optional_ids_are_parsed_when_set() {
        let config = Config::from_lookup(|name| match name {
            OWNER_ID_ENV_VAR => Some("303".to_owned()),
            GITHUB_EMOJI_ID_ENV_VAR => Some(" 404 ".to_owned()),
            INVITE_URL_ENV_VAR => Some(" https://discord.gg/example ".to_owned()),
            _ => configured(name),
        })
        .expect("configuration accepted");

        assert_eq!(config.ids.owner, Some(UserId::new(303)));
        assert_eq!(config.ids.github_emoji, Some(EmojiId::new(404)));
        assert_eq!(
            config.dashboard.invite_url.as_deref(),
            Some("https://discord.gg/example")
        );
    }

    #[test]
    fn blank_optional_values_are_ignored() {
        let config = Config::from_lookup(|name| match name {
            OWNER_ID_ENV_VAR | GITHUB_EMOJI_ID_ENV_VAR | INVITE_URL_ENV_VAR => {
                Some("   ".to_owned())
            }
            _ => configured(name),
        })
        .expect("configuration accepted");

        assert_eq!(config.ids.owner, None);
        assert_eq!(config.ids.github_emoji, None);
        assert_eq!(config.dashboard.invite_url, None);
    }

    #[test]
    fn zero_optional_id_is_rejected() {
        let err = Config::from_lookup(|name| match name {
            OWNER_ID_ENV_VAR => Some("0".to_owned()),
            _ => configured(name),
        })
        .expect_err("zero owner id rejected");

        assert!(err.contains(OWNER_ID_ENV_VAR));
    }

    #[test]
    fn a_non_http_invite_url_is_rejected() {
        let err = Config::from_lookup(|name| match name {
            INVITE_URL_ENV_VAR => Some("discord.gg/example".to_owned()),
            _ => configured(name),
        })
        .expect_err("non-URL rejected");

        assert!(err.contains(INVITE_URL_ENV_VAR));
    }

    #[test]
    fn an_uppercase_invite_scheme_is_accepted() {
        let config = Config::from_lookup(|name| match name {
            INVITE_URL_ENV_VAR => Some("HTTPS://discord.gg/example".to_owned()),
            _ => configured(name),
        })
        .expect("configuration accepted");

        assert_eq!(
            config.dashboard.invite_url.as_deref(),
            Some("HTTPS://discord.gg/example")
        );
    }

    #[test]
    fn an_invite_url_without_a_host_is_rejected() {
        let err = Config::from_lookup(|name| match name {
            INVITE_URL_ENV_VAR => Some("https://".to_owned()),
            _ => configured(name),
        })
        .expect_err("host-less URL rejected");

        assert!(err.contains(INVITE_URL_ENV_VAR));
    }

    #[test]
    fn debug_does_not_leak_the_token() {
        let config = Config::from_lookup(configured).expect("configuration accepted");
        let rendered = format!("{config:?}");

        assert!(!rendered.contains("abc.def.ghi"));
        assert!(rendered.contains("redacted"));
    }
}
