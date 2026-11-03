//! `/role`: add a role to, or remove a role from, a guild member.
//!
//! The command is interactive: the `role` option is a string option with
//! autocomplete enabled, so the role picker is populated from the guild cache
//! as the user types (`docs/application-commands.md`, Autocomplete). The result
//! reply is an ephemeral embed with an **Undo** button that reverses the change;
//! clicking it edits that same message (`docs/receiving-and-responding.md`,
//! Interaction Callback Type, `UPDATE_MESSAGE`).
//!
//! Safety rules, all enforced before any REST call and re-enforced when the undo
//! button is clicked:
//!
//! * the invoking member needs `MANAGE_ROLES` (the registration's
//!   `default_member_permissions` is only an overridable default, so the real
//!   check is the fail-closed gate in [`crate::options`]);
//! * the target cannot be the guild owner, the bot itself, or someone whose
//!   highest role is not below the bot's highest role;
//! * the role cannot be `@everyone`, cannot be managed by an integration, and
//!   its position must be below the bot's highest role;
//! * when the bot's member object is not cached, the hierarchy is left to
//!   Discord to enforce rather than guessing.
//!
//! Reference: `docs/guild.md` (Add Guild Member Role, Remove Guild Member Role),
//! `docs/permissions.md` (Permission Hierarchy), `docs/components-reference.md`
//! (Action Row, Button, `custom_id` limits) and
//! `docs/receiving-and-responding.md` (Message Component interactions).

use discord::builder::command::CommandBuilder;
use discord::builder::command::CommandOptionBuilder;
use discord::builder::embed::EmbedBuilder;
use discord::builder::interaction::ActionRowBuilder;
use discord::builder::interaction::AutocompleteChoice;
use discord::builder::interaction::ButtonBuilder;
use discord::builder::interaction::InteractionEditBuilder;
use discord::builder::interaction::ReplyBuilder;
use discord::builder::interaction::UpdateMessageBuilder;
use discord::builder::message::AllowedMentionsBuilder;
use discord::client::Context;
use discord::id::RoleId;
use discord::id::UserId;
use discord::model::application::AutocompleteInteraction;
use discord::model::application::CommandInteraction;
use discord::model::application::CommandOptionKind;
use discord::model::application::ComponentInteraction;
use discord::model::guild::Guild;
use discord::model::guild::Member;
use discord::model::guild::Role;
use discord::model::message::Embed;
use discord::permissions::Permissions;

use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::error::AppError;
use crate::error::UNSUPPORTED_MESSAGE;
use crate::options::ensure_component_permission;
use crate::options::ensure_permission;
use crate::options::required_string;
use crate::options::required_user;

/// Prefix of the Undo button's `custom_id`; the full format is
/// `role-undo:<user_id>:<role_id>:<add|remove>`.
///
/// Reference: `docs/components-reference.md` (Anatomy of a Component, Custom
/// ID) — a `custom_id` is 1-100 characters and identifies the component when it
/// is clicked.
const UNDO_PREFIX: &str = "role-undo:";

/// Maximum characters in a `custom_id`.
///
/// Reference: `docs/components-reference.md` (Anatomy of a Component, Custom
/// ID: "a string of 1 to 100 characters").
const CUSTOM_ID_MAX: usize = 100;

/// Maximum autocomplete choices in one response.
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Data
/// Structure, Autocomplete: "max of 25 choices").
const AUTOCOMPLETE_LIMIT: usize = 25;

/// Embed colour for a role that was added (Discord green).
const COLOR_ADDED: u32 = 0x0057_F287;

/// Embed colour for a role that was removed (Discord red).
const COLOR_REMOVED: u32 = 0x00ED_4245;

/// Embed colour for a change that was undone (Discord blurple).
const COLOR_UNDONE: u32 = 0x0058_65F2;

/// What `/role` does to the target's role list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoleAction {
    /// Add the role to the member.
    Add,
    /// Remove the role from the member.
    Remove,
}

impl RoleAction {
    /// Parses the `execute` option, case-insensitively.
    ///
    /// Only `add` and `remove` are accepted; everything else is rejected so a
    /// typo never becomes a silent no-op (`docs/application-commands.md`,
    /// Application Command Option Structure).
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "add" => Some(Self::Add),
            "remove" => Some(Self::Remove),
            _ => None,
        }
    }

    /// Returns the wire spelling used in the `custom_id`.
    const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Remove => "remove",
        }
    }

    /// Returns the verb used in descriptions.
    const fn past_tense(self) -> &'static str {
        match self {
            Self::Add => "Added",
            Self::Remove => "Removed",
        }
    }

    /// Returns the preposition used in descriptions.
    const fn preposition(self) -> &'static str {
        match self {
            Self::Add => "to",
            Self::Remove => "from",
        }
    }

    /// Returns the action that undoes this one.
    const fn reversed(self) -> Self {
        match self {
            Self::Add => Self::Remove,
            Self::Remove => Self::Add,
        }
    }

    /// Returns the result embed title.
    const fn title(self) -> &'static str {
        match self {
            Self::Add => "Role added",
            Self::Remove => "Role removed",
        }
    }

    /// Returns the result embed colour.
    const fn color(self) -> u32 {
        match self {
            Self::Add => COLOR_ADDED,
            Self::Remove => COLOR_REMOVED,
        }
    }
}

/// Parses the `execute` option into a [`RoleAction`].
///
/// # Errors
///
/// [`AppError::Denied`] when the value is neither `add` nor `remove`; the
/// message tells the user what is accepted.
fn parse_execute(raw: &str) -> Result<RoleAction, AppError> {
    RoleAction::parse(raw).ok_or(AppError::Denied(
        "The 'execute' option must be either 'add' or 'remove'.",
    ))
}

/// One encoded Undo button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UndoAction {
    /// The member whose roles were changed.
    user_id: UserId,
    /// The role that was added or removed.
    role_id: RoleId,
    /// The action the button undoes.
    performed: RoleAction,
}

/// Encodes the Undo button's `custom_id`.
fn encode_undo(undo: UndoAction) -> String {
    format!(
        "{UNDO_PREFIX}{}:{}:{}",
        undo.user_id,
        undo.role_id,
        undo.performed.as_str()
    )
}

/// Decodes an Undo button's `custom_id`, rejecting anything malformed.
///
/// Returns `None` for a wrong prefix, a wrong number of fields, a non-numeric or
/// zero snowflake, an unknown action, or a value over the documented
/// [`CUSTOM_ID_MAX`] limit. The button is remote data: a stale or edited
/// `custom_id` must never panic or be trusted.
fn decode_undo(custom_id: &str) -> Option<UndoAction> {
    if custom_id.chars().count() > CUSTOM_ID_MAX {
        return None;
    }

    let rest = custom_id.strip_prefix(UNDO_PREFIX)?;
    let mut fields = rest.split(':');

    let user_id = snowflake(fields.next()?)?;
    let role_id = snowflake(fields.next()?)?;
    let performed = RoleAction::parse(fields.next()?)?;

    // A well-formed id has exactly three fields after the prefix.
    if fields.next().is_some() {
        return None;
    }

    Some(UndoAction {
        user_id: UserId::new(user_id),
        role_id: RoleId::new(role_id),
        performed,
    })
}

/// Parses a decimal snowflake, rejecting zero (Discord ids are never zero).
fn snowflake(raw: &str) -> Option<u64> {
    raw.parse::<u64>().ok().filter(|value| *value != 0)
}

/// Returns whether `custom_id` claims to be a `/role` Undo button.
///
/// The component dispatcher uses this to route before decoding, so an unrelated
/// button is answered with the generic fallback instead of a parse error.
#[must_use]
pub(crate) fn is_undo_custom_id(custom_id: &str) -> bool {
    custom_id.starts_with(UNDO_PREFIX)
}

/// Resolves the `role` option to a cached guild role.
///
/// The autocomplete choice value is the role id as a decimal string, but a user
/// may also type a role name by hand (autocomplete is a suggestion, not a
/// constraint), so an exact case-insensitive name is accepted too. An ambiguous
/// name is rejected instead of guessing.
///
/// # Errors
///
/// [`AppError::Denied`] when no role matches, or when a name matches more than
/// one role.
fn resolve_role(guild: &Guild, raw: &str) -> Result<RoleId, AppError> {
    let raw = raw.trim();

    if let Some(id) = snowflake(raw) {
        let role_id = RoleId::new(id);

        if guild.roles.contains_key(&role_id) {
            return Ok(role_id);
        }

        return Err(role_not_found());
    }

    let mut matches = guild
        .roles
        .values()
        .filter(|role| role.id.get() != guild.id.get() && role.name.eq_ignore_ascii_case(raw));

    let Some(role) = matches.next() else {
        return Err(role_not_found());
    };

    if matches.next().is_some() {
        return Err(AppError::Denied(
            "More than one role has that name; pick one from the autocomplete list.",
        ));
    }

    Ok(role.id)
}

/// The error shown when the requested role is not in the guild.
fn role_not_found() -> AppError {
    AppError::Denied("That role does not exist in this server.")
}

/// Builds the autocomplete choices for the `role` option.
///
/// The cache is the only data source and no REST call is made, so the response
/// comfortably fits Discord's 3-second autocomplete deadline
/// (`docs/receiving-and-responding.md`, Interaction Callback).
///
/// Returns an empty list (rather than failing) when the guild is not cached or
/// the focused option is not the one this bot autocompletes.
pub(crate) fn autocomplete_choices(
    ctx: &Context,
    interaction: &AutocompleteInteraction,
) -> Vec<AutocompleteChoice> {
    let Some(guild_id) = interaction.guild_id else {
        debug!("ignoring an autocomplete interaction outside a guild");
        return Vec::new();
    };

    let Some(guild) = ctx.cache.guild(guild_id) else {
        warn!(%guild_id, "autocomplete could not read the guild from the cache");
        return Vec::new();
    };

    let Some(focused) = interaction.focused_option() else {
        warn!(
            command = %interaction.data.name,
            "autocomplete interaction has no focused option"
        );
        return Vec::new();
    };

    if focused.name != "role" {
        debug!(option = %focused.name, "ignoring a focused option with no autocomplete handler");
        return Vec::new();
    }

    let query = focused.value.as_str().unwrap_or_default();

    role_choices(&guild, query)
}

/// Builds role choices from the cache: `@everyone` excluded, filtered by a
/// case-insensitive substring of `query`, highest position first, at most
/// [`AUTOCOMPLETE_LIMIT`] entries.
#[must_use]
fn role_choices(guild: &Guild, query: &str) -> Vec<AutocompleteChoice> {
    let query = query.trim().to_ascii_lowercase();

    let mut roles: Vec<&Role> = guild
        .roles
        .values()
        .filter(|role| role.id.get() != guild.id.get())
        .filter(|role| query.is_empty() || role.name.to_ascii_lowercase().contains(&query))
        .collect();

    roles.sort_by(|left, right| {
        right
            .position
            .cmp(&left.position)
            .then_with(|| left.name.cmp(&right.name))
    });

    roles
        .into_iter()
        .take(AUTOCOMPLETE_LIMIT)
        .map(|role| AutocompleteChoice::new(role.name.clone(), role.id.to_string()))
        .collect()
}

/// Returns the rank `(position, id)` of the member's highest role, with
/// `@everyone` (position 0) as the floor.
///
/// Reference: `docs/permissions.md` (Permission Hierarchy) — "A bot's or user's
/// highest role is its role that has the greatest position value in the guild,
/// with the default @everyone role starting at 0." Roles with the same position
/// sort by id (`docs/permissions.md`, Role Object), so the tie-break is part of
/// the rank.
#[must_use]
fn highest_role_rank(guild: &Guild, member: &Member) -> (i64, u64) {
    let mut rank = (0_i64, guild.id.get());

    for role_id in &member.roles {
        let Some(role) = guild.roles.get(role_id) else {
            continue;
        };

        // A greater position is higher; at an equal position the lower id is
        // higher (roles with the same position sort by id).
        if role.position > rank.0 || (role.position == rank.0 && role.id.get() < rank.1) {
            rank = (role.position, role.id.get());
        }
    }

    rank
}

/// Returns true when `(position, id)` sits below `(top_position, top_id)` in
/// Discord's role hierarchy.
///
/// Roles with the same position sort by id (`docs/permissions.md`, Role
/// Object), so at an equal position a *greater* id is lower and can be managed
/// by a bot whose highest role is `top_position`/`top_id`.
#[must_use]
const fn is_lower_in_hierarchy(position: i64, id: u64, top_position: i64, top_id: u64) -> bool {
    position < top_position || (position == top_position && id > top_id)
}

/// Validates that the bot may add or remove `role` for `target`.
///
/// Everything here is cache-only, so it runs before the interaction is
/// acknowledged: a refusal is shown immediately, and nothing is changed.
///
/// Reference: `docs/permissions.md` (Permission Hierarchy) and
/// `docs/guild.md` (Add Guild Member Role, Remove Guild Member Role).
///
/// # Errors
///
/// [`AppError::Denied`] with a user-facing explanation when the change is not
/// allowed: `@everyone`, a managed role, the guild owner, the bot itself, or a
/// role/member above the bot in the hierarchy.
fn validate_role_change(
    guild: &Guild,
    role: &Role,
    target: UserId,
    bot: UserId,
) -> Result<(), AppError> {
    if role.id.get() == guild.id.get() {
        return Err(AppError::Denied(
            "@everyone cannot be added or removed; it is part of every member.",
        ));
    }

    if role.managed {
        return Err(AppError::Denied(
            "That role is managed by an integration and cannot be assigned manually.",
        ));
    }

    if guild.is_owner(target) {
        return Err(AppError::Denied(
            "The server owner's roles cannot be changed.",
        ));
    }

    if target == bot {
        return Err(AppError::Denied("I cannot change my own roles."));
    }

    // The hierarchy is only knowable when the bot's member object is cached.
    // Without it, Discord is the authority; with it, the documented rules are
    // enforced here so the user gets a clear message instead of an API error.
    let Some(bot_member) = guild.members.get(&bot) else {
        return Ok(());
    };

    let (bot_top_position, bot_top_id) = highest_role_rank(guild, bot_member);

    if !is_lower_in_hierarchy(role.position, role.id.get(), bot_top_position, bot_top_id) {
        return Err(AppError::Denied(
            "That role is not lower than my highest role, so I cannot manage it.",
        ));
    }

    if let Some(target_member) = guild.members.get(&target) {
        let (target_position, target_id) = highest_role_rank(guild, target_member);

        if !is_lower_in_hierarchy(target_position, target_id, bot_top_position, bot_top_id) {
            return Err(AppError::Denied(
                "That member's highest role is not lower than mine, so I cannot manage their roles.",
            ));
        }
    }

    Ok(())
}

/// Returns the bot's own user id from the cache.
///
/// # Errors
///
/// [`AppError::Denied`] while `READY` has not populated the cache yet.
fn bot_id(ctx: &Context) -> Result<UserId, AppError> {
    ctx.cache
        .current_user()
        .map(|user| user.id)
        .ok_or(AppError::Denied(
            "The bot is still starting up, please try again in a moment.",
        ))
}

/// Builds the result embed shown for a successful change.
#[must_use]
fn result_embed(action: RoleAction, role_name: &str, target: UserId) -> Embed {
    EmbedBuilder::new()
        .title(action.title())
        .description(format!(
            "{} **{}** {} <@{}>.",
            action.past_tense(),
            role_name,
            action.preposition(),
            target
        ))
        .color(action.color())
        .build()
}

/// Builds the result embed shown after an Undo click.
#[must_use]
fn undone_embed(reverted: RoleAction, role_name: &str, target: UserId) -> Embed {
    EmbedBuilder::new()
        .title("Role change undone")
        .description(format!(
            "Reverted the last change: {} **{}** {} <@{}>.",
            reverted.past_tense(),
            role_name,
            reverted.preposition(),
            target
        ))
        .color(COLOR_UNDONE)
        .build()
}

/// Builds the Undo button row for `undo`.
#[must_use]
fn undo_row(undo: UndoAction) -> ActionRowBuilder {
    ActionRowBuilder::new().button(ButtonBuilder::secondary(encode_undo(undo), "Undo"))
}

/// Answers a stale or malformed `/role` Undo click so the user never sees
/// Discord's "This interaction failed".
async fn answer_unsupported(ctx: &Context, interaction: &ComponentInteraction) {
    let response = ReplyBuilder::new().content(UNSUPPORTED_MESSAGE).ephemeral();

    if let Err(err) = interaction.respond(&ctx.http, response).await {
        error!(
            "failed to answer an unsupported /role undo interaction '{}': {}",
            interaction.data.custom_id, err
        );
    }
}

/// Runs `/role`.
///
/// # Errors
///
/// [`AppError::Denied`] when the member lacks `MANAGE_ROLES` or a documented
/// safety rule rejects the change, and [`AppError::Discord`] when a REST call
/// fails.
pub(crate) async fn run(ctx: &Context, interaction: &CommandInteraction) -> Result<(), AppError> {
    // The real authorization boundary: `default_member_permissions` on the
    // registration is only a default a guild can override.
    ensure_permission(ctx, interaction, Permissions::MANAGE_ROLES)?;

    let target: UserId = required_user(interaction, "target")?;
    let action = parse_execute(required_string(interaction, "execute")?)?;
    let raw_role: &str = required_string(interaction, "role")?;

    let guild_id = interaction.guild_id.ok_or(AppError::Denied(
        "This command can only be used inside a server.",
    ))?;

    let guild = ctx.cache.guild(guild_id).ok_or(AppError::Denied(
        "The server is still loading, please try again in a moment.",
    ))?;

    let role_id = resolve_role(&guild, raw_role)?;
    let Some(role) = guild.roles.get(&role_id) else {
        return Err(role_not_found());
    };

    let bot = bot_id(ctx)?;
    validate_role_change(&guild, role, target, bot)?;

    let role_name = role.name.clone();

    // Everything above is cache-only, so the response can be built before the
    // interaction is acknowledged. That keeps the 3-second deadline: only the
    // deferred call and the role REST call happen after this point.
    let embed = result_embed(action, &role_name, target);
    let undo = UndoAction {
        user_id: target,
        role_id,
        performed: action,
    };
    let row = undo_row(undo);
    let reason = format!("/role {} by {}", action.as_str(), interaction.user.username);

    interaction.defer_ephemeral(&ctx.http).await?;

    match action {
        RoleAction::Add => {
            ctx.http
                .add_member_role(guild_id, target, role_id, Some(&reason))
                .await?;
        }
        RoleAction::Remove => {
            ctx.http
                .remove_member_role(guild_id, target, role_id, Some(&reason))
                .await?;
        }
    }

    info!(
        "Role '{}' ({}) was {} for {}[{}] via /role (invoked by {}[{}]).",
        role_name,
        role_id,
        action.past_tense().to_ascii_lowercase(),
        target,
        target,
        interaction.user.username,
        interaction.user.id
    );

    let response = InteractionEditBuilder::new()
        .embed(embed)
        .components([row])
        .allowed_mentions(AllowedMentionsBuilder::none());

    interaction.edit_response(&ctx.http, response).await?;

    Ok(())
}

/// Handles a click on a `/role` Undo button.
///
/// The permission gate is re-run for the member who *clicked*, not the member who
/// originally ran the command: a button outlives the invoker's access, so it is
/// not an authorization boundary. The change is then reversed and the original
/// message is edited in place (callback type 7).
///
/// Reference: `docs/receiving-and-responding.md` (Interaction Callback Type,
/// `UPDATE_MESSAGE`) and `docs/permissions.md` (Permission Hierarchy).
///
/// # Errors
///
/// [`AppError::Denied`] when the clicking member lost `MANAGE_ROLES` or the
/// change is no longer allowed, and [`AppError::Discord`] when a REST call fails.
pub(crate) async fn undo(
    ctx: &Context,
    interaction: &ComponentInteraction,
) -> Result<(), AppError> {
    let Some(record) = decode_undo(&interaction.data.custom_id) else {
        warn!(
            custom_id = %interaction.data.custom_id,
            "rejecting a malformed /role undo button"
        );
        answer_unsupported(ctx, interaction).await;

        return Ok(());
    };

    ensure_component_permission(ctx, interaction, Permissions::MANAGE_ROLES)?;

    let guild_id = interaction
        .guild_id
        .ok_or(AppError::Denied("This can only be used inside a server."))?;

    let guild = ctx.cache.guild(guild_id).ok_or(AppError::Denied(
        "The server is still loading, please try again in a moment.",
    ))?;

    let Some(role) = guild.roles.get(&record.role_id) else {
        warn!(
            role = %record.role_id,
            "the role in a /role undo button no longer exists"
        );
        answer_unsupported(ctx, interaction).await;

        return Ok(());
    };

    let bot = bot_id(ctx)?;
    validate_role_change(&guild, role, record.user_id, bot)?;

    let role_name = role.name.clone();
    let reverted = record.performed.reversed();

    let embed = undone_embed(reverted, &role_name, record.user_id);
    let next_undo = UndoAction {
        performed: reverted,
        ..record
    };
    let row = undo_row(next_undo);
    let reason = format!(
        "/role undo {} by {}",
        record.performed.as_str(),
        interaction.user.username
    );

    match reverted {
        RoleAction::Add => {
            ctx.http
                .add_member_role(guild_id, record.user_id, record.role_id, Some(&reason))
                .await?;
        }
        RoleAction::Remove => {
            ctx.http
                .remove_member_role(guild_id, record.user_id, record.role_id, Some(&reason))
                .await?;
        }
    }

    info!(
        "Undid /role {} on {}[{}] for role '{}' ({}) (clicked by {}[{}]).",
        record.performed.as_str(),
        record.user_id,
        record.user_id,
        role_name,
        record.role_id,
        interaction.user.username,
        interaction.user.id
    );

    let response = UpdateMessageBuilder::new()
        .embed(embed)
        .components([row])
        .allowed_mentions(AllowedMentionsBuilder::none());

    interaction.update_message(&ctx.http, response).await?;

    Ok(())
}

/// Registers `/role`.
///
/// `role` is a *string* option with autocomplete enabled: Discord only allows
/// autocomplete on string, integer and number options
/// (`docs/application-commands.md`, Application Command Option Structure), and
/// the autocomplete result values are role ids encoded as strings. `execute` is
/// a string option with the fixed choices `add`/`remove`, so Discord's UI only
/// offers those two values (`docs/application-commands.md`, Application Command
/// Option Choice Structure).
pub(crate) fn register() -> CommandBuilder {
    CommandBuilder::new("role", "Adds or removes a role from a member.")
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::User,
                "target",
                "The member whose roles you would like to edit.",
            )
            .required(true),
        )
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::Str,
                "role",
                "The role to add or remove (start typing for suggestions).",
            )
            .required(true)
            .autocomplete(true),
        )
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::Str,
                "execute",
                "What to do: `add` or `remove`.",
            )
            .required(true)
            .choice("Add", "add")
            .choice("Remove", "remove"),
        )
        .default_member_permissions(Permissions::MANAGE_ROLES)
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use serde_json::json;

    use super::*;

    /// Builds a guild fixture from role and member JSON, always adding the
    /// `@everyone` role whose id equals the guild id.
    fn guild_with(roles: Vec<Value>, members: &[Value], owner_id: u64) -> Guild {
        let mut all_roles = vec![json!({
            "id": "1",
            "name": "@everyone",
            "position": 0,
            "permissions": "0",
        })];
        all_roles.extend(roles);

        serde_json::from_value(json!({
            "id": "1",
            "name": "Test Guild",
            "owner_id": owner_id.to_string(),
            "roles": all_roles,
            "members": members,
        }))
        .expect("the fixture matches the documented guild shape")
    }

    fn role_json(id: u64, name: &str, position: i64) -> Value {
        json!({
            "id": id.to_string(),
            "name": name,
            "color": 0,
            "hoist": false,
            "position": position,
            "permissions": "0",
            "managed": false,
            "mentionable": false,
        })
    }

    fn member_json(id: u64, roles: &[u64]) -> Value {
        json!({
            "user": {"id": id.to_string(), "username": format!("user{id}"), "discriminator": "0"},
            "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
            "joined_at": "2020-01-01T00:00:00.000000+00:00",
            "deaf": false,
            "mute": false,
        })
    }

    /// A guild where the bot (1000) has Helper (position 3), the target (2000)
    /// has no roles, and the owner is 100.
    fn standard_guild() -> Guild {
        guild_with(
            vec![
                role_json(10, "Admin", 5),
                role_json(20, "Moderator", 4),
                role_json(30, "Helper", 3),
                role_json(40, "Muted", 1),
            ],
            &[member_json(1000, &[30]), member_json(2000, &[])],
            100,
        )
    }

    fn role(guild: &Guild, id: u64) -> Role {
        guild
            .roles
            .get(&RoleId::new(id))
            .cloned()
            .expect("the fixture role exists")
    }

    // --- execute parsing -------------------------------------------------

    #[test]
    fn execute_parsing_is_case_insensitive_and_trimmed() {
        assert_eq!(RoleAction::parse("add"), Some(RoleAction::Add));
        assert_eq!(RoleAction::parse("ADD"), Some(RoleAction::Add));
        assert_eq!(RoleAction::parse(" Add "), Some(RoleAction::Add));
        assert_eq!(RoleAction::parse("remove"), Some(RoleAction::Remove));
        assert_eq!(RoleAction::parse("Remove"), Some(RoleAction::Remove));
    }

    #[test]
    fn execute_parsing_rejects_anything_else() {
        for raw in ["", "grant", "added", "remove please", "list"] {
            assert_eq!(RoleAction::parse(raw), None, "input {raw:?}");
        }

        let err = parse_execute("grant").expect_err("grant is not an accepted action");
        assert!(matches!(err, AppError::Denied(_)));
        assert!(err.to_string().contains("'add' or 'remove'"));
    }

    #[test]
    fn role_action_descriptions_and_reversal_are_consistent() {
        assert_eq!(RoleAction::Add.as_str(), "add");
        assert_eq!(RoleAction::Remove.as_str(), "remove");
        assert_eq!(RoleAction::Add.past_tense(), "Added");
        assert_eq!(RoleAction::Remove.past_tense(), "Removed");
        assert_eq!(RoleAction::Add.preposition(), "to");
        assert_eq!(RoleAction::Remove.preposition(), "from");
        assert_eq!(RoleAction::Add.title(), "Role added");
        assert_eq!(RoleAction::Remove.title(), "Role removed");
        assert_eq!(RoleAction::Add.color(), COLOR_ADDED);
        assert_eq!(RoleAction::Remove.color(), COLOR_REMOVED);
        assert_eq!(RoleAction::Add.reversed(), RoleAction::Remove);
        assert_eq!(RoleAction::Remove.reversed(), RoleAction::Add);
    }

    // --- undo custom_id --------------------------------------------------

    fn undo(user: u64, role: u64, performed: RoleAction) -> UndoAction {
        UndoAction {
            user_id: UserId::new(user),
            role_id: RoleId::new(role),
            performed,
        }
    }

    #[test]
    fn undo_custom_id_round_trips() {
        for performed in [RoleAction::Add, RoleAction::Remove] {
            let original = undo(2000, 40, performed);
            let encoded = encode_undo(original);

            assert_eq!(decode_undo(&encoded), Some(original), "encoded {encoded:?}");
        }
    }

    #[test]
    fn undo_custom_id_encodes_the_documented_format() {
        assert_eq!(
            encode_undo(undo(2000, 40, RoleAction::Add)),
            "role-undo:2000:40:add"
        );
        assert_eq!(
            encode_undo(undo(2000, 40, RoleAction::Remove)),
            "role-undo:2000:40:remove"
        );
    }

    #[test]
    fn undo_custom_id_stays_under_the_documented_length_limit() {
        let longest = undo(u64::MAX, u64::MAX, RoleAction::Remove);
        let encoded = encode_undo(longest);

        assert!(
            encoded.chars().count() <= CUSTOM_ID_MAX,
            "{encoded:?} is {} chars",
            encoded.chars().count()
        );
        assert_eq!(decode_undo(&encoded), Some(longest));
    }

    #[test]
    fn undo_custom_id_rejects_malformed_values() {
        for custom_id in [
            "",
            "role-undo",
            "role-undo:",
            "role-undo:2000:40",
            "role-undo::40:add",
            "role-undo:abc:40:add",
            "role-undo:0:40:add",
            "role-undo:2000:abc:add",
            "role-undo:2000:0:add",
            "role-undo:2000:40:grant",
            "role-undo:2000:40:add:extra",
            "role-undo:-1:40:add",
            "other-prefix:2000:40:add",
            "role-undo:2000:40:add trailing",
        ] {
            assert_eq!(decode_undo(custom_id), None, "input {custom_id:?}");
        }
    }

    #[test]
    fn undo_custom_id_rejects_oversized_values() {
        let mut custom_id = encode_undo(undo(2000, 40, RoleAction::Add));
        while custom_id.chars().count() <= CUSTOM_ID_MAX {
            custom_id.push('0');
        }

        assert_eq!(decode_undo(&custom_id), None);
    }

    #[test]
    fn is_undo_custom_id_matches_only_the_prefix() {
        assert!(is_undo_custom_id("role-undo:1:2:add"));
        assert!(is_undo_custom_id(UNDO_PREFIX));
        assert!(!is_undo_custom_id("role-undo"));
        assert!(!is_undo_custom_id("role-undoing:1:2:add"));
        assert!(!is_undo_custom_id("role-info:1:2:add"));
    }

    // --- role resolution -------------------------------------------------

    #[test]
    fn resolves_a_role_from_its_autocomplete_value() {
        let guild = standard_guild();

        assert_eq!(
            resolve_role(&guild, "40").expect("role id resolves"),
            RoleId::new(40)
        );
    }

    #[test]
    fn resolves_a_role_from_an_exact_name_case_insensitively() {
        let guild = standard_guild();

        assert_eq!(
            resolve_role(&guild, "Muted").expect("exact name resolves"),
            RoleId::new(40)
        );
        assert_eq!(
            resolve_role(&guild, "muted").expect("lower-case name resolves"),
            RoleId::new(40)
        );
        assert_eq!(
            resolve_role(&guild, "  Muted ").expect("trimmed name resolves"),
            RoleId::new(40)
        );
    }

    #[test]
    fn rejects_an_unknown_role_id_or_name() {
        let guild = standard_guild();

        for raw in ["999", "Missing", ""] {
            let err = resolve_role(&guild, raw).expect_err("unknown role is rejected");
            assert!(matches!(err, AppError::Denied(_)), "input {raw:?}");
            assert!(err.to_string().contains("does not exist"));
        }
    }

    #[test]
    fn rejects_an_ambiguous_role_name_instead_of_guessing() {
        let guild = guild_with(
            vec![role_json(10, "Team", 2), role_json(20, "Team", 1)],
            &[],
            100,
        );

        let err = resolve_role(&guild, "Team").expect_err("ambiguous name is rejected");
        assert!(matches!(err, AppError::Denied(_)));
        assert!(err.to_string().contains("More than one role"));
    }

    #[test]
    fn does_not_resolve_everyone_by_name() {
        let guild = standard_guild();

        let err = resolve_role(&guild, "@everyone").expect_err("@everyone is not assignable");
        assert!(err.to_string().contains("does not exist"));
    }

    // --- autocomplete choices -------------------------------------------

    #[test]
    fn choices_exclude_everyone_and_are_sorted_by_position() {
        let guild = standard_guild();

        let choices = role_choices(&guild, "");

        assert_eq!(choices.len(), 4);
        assert_eq!(
            choices
                .first()
                .map(|choice| choice.to_json()["value"].clone()),
            Some(json!("10")),
            "Admin has the highest position"
        );
        assert!(
            choices
                .iter()
                .all(|choice| choice.to_json()["value"] != json!("1")),
            "@everyone must never be suggested"
        );
    }

    #[test]
    fn choices_filter_by_a_case_insensitive_substring() {
        let guild = standard_guild();

        let choices = role_choices(&guild, "MOD");

        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].to_json()["name"], json!("Moderator"));
        assert_eq!(choices[0].to_json()["value"], json!("20"));
    }

    #[test]
    fn choices_with_no_match_are_empty() {
        let guild = standard_guild();

        assert!(role_choices(&guild, "does-not-exist").is_empty());
    }

    #[test]
    fn choices_are_capped_at_the_documented_limit() {
        let roles: Vec<Value> = (0..30)
            .map(|index| {
                role_json(
                    1_000 + index,
                    &format!("Role {index:02}"),
                    i64::try_from(index).expect("small test index"),
                )
            })
            .collect();
        let guild = guild_with(roles, &[], 100);

        let choices = role_choices(&guild, "");

        assert_eq!(choices.len(), AUTOCOMPLETE_LIMIT);
        // Highest position first, so the 30th (position 29) is at the front and
        // the cap drops the lowest positions.
        assert_eq!(choices[0].to_json()["name"], json!("Role 29"));
        assert_eq!(choices[24].to_json()["name"], json!("Role 05"));
    }

    #[test]
    fn choices_serialise_the_name_and_role_id_value() {
        let guild = standard_guild();

        let choices = role_choices(&guild, "muted");

        assert_eq!(
            choices[0].to_json(),
            json!({"name": "Muted", "value": "40"}),
            "the autocomplete value must be the role id as a string"
        );
    }

    #[test]
    fn choices_are_empty_when_there_are_no_roles_other_than_everyone() {
        let guild = guild_with(vec![], &[], 100);

        assert!(role_choices(&guild, "").is_empty());
    }

    // --- hierarchy checks -------------------------------------------------

    #[test]
    fn highest_role_rank_uses_the_highest_assigned_role() {
        let guild = standard_guild();
        let bot = member_json(1000, &[30]);
        let bot: Member = serde_json::from_value(bot).expect("member fixture");

        assert_eq!(highest_role_rank(&guild, &bot), (3, 30));
    }

    #[test]
    fn highest_role_rank_without_roles_is_everyone() {
        let guild = standard_guild();
        let target: Member =
            serde_json::from_value(member_json(2000, &[])).expect("member fixture");

        assert_eq!(highest_role_rank(&guild, &target), (0, 1));
    }

    #[test]
    fn highest_role_rank_breaks_position_ties_by_id() {
        // Roles 29, 30 and 31 share position 3: the lower id is higher, so 29 is
        // the member's highest role (`docs/permissions.md`, Role Object).
        let guild = guild_with(
            vec![
                role_json(29, "First", 3),
                role_json(30, "Second", 3),
                role_json(31, "Third", 3),
                role_json(40, "Muted", 1),
            ],
            &[member_json(1000, &[29, 30, 31, 40])],
            100,
        );
        let member: Member =
            serde_json::from_value(member_json(1000, &[29, 30, 31, 40])).expect("member fixture");

        assert_eq!(highest_role_rank(&guild, &member), (3, 29));
    }

    #[test]
    fn a_role_at_the_same_position_is_manageable_when_its_id_is_lower() {
        // Discord sorts roles with the same position by id (`docs/permissions.md`,
        // Role Object): at the bot's own position, a greater id is lower in the
        // hierarchy and can be managed.
        let guild = guild_with(
            vec![
                role_json(30, "Helper", 3),
                role_json(31, "Same Position", 3),
                role_json(40, "Muted", 1),
            ],
            &[member_json(1000, &[30]), member_json(2000, &[])],
            100,
        );
        let same_position = role(&guild, 31);

        assert!(
            validate_role_change(&guild, &same_position, UserId::new(2000), UserId::new(1000))
                .is_ok()
        );
    }

    #[test]
    fn a_role_at_the_same_position_is_rejected_when_its_id_is_higher() {
        let guild = guild_with(
            vec![
                role_json(29, "Same Position", 3),
                role_json(30, "Helper", 3),
                role_json(40, "Muted", 1),
            ],
            &[member_json(1000, &[30]), member_json(2000, &[])],
            100,
        );
        let same_position = role(&guild, 29);

        let err =
            validate_role_change(&guild, &same_position, UserId::new(2000), UserId::new(1000))
                .expect_err("same-position role with a lower id outranks the bot");
        assert!(err.to_string().contains("not lower than my highest role"));
    }

    #[test]
    fn a_target_at_the_same_position_is_manageable_when_its_id_is_lower() {
        let guild = guild_with(
            vec![
                role_json(30, "Helper", 3),
                role_json(31, "Same Position", 3),
                role_json(40, "Muted", 1),
            ],
            &[member_json(1000, &[30]), member_json(2000, &[31])],
            100,
        );
        let muted = role(&guild, 40);

        assert!(validate_role_change(&guild, &muted, UserId::new(2000), UserId::new(1000)).is_ok());
    }

    #[test]
    fn rejects_everyone() {
        let guild = standard_guild();
        let everyone = role(&guild, 1);

        let err = validate_role_change(&guild, &everyone, UserId::new(2000), UserId::new(1000))
            .expect_err("@everyone is rejected");
        assert!(err.to_string().contains("@everyone"));
    }

    #[test]
    fn rejects_managed_roles() {
        let guild = guild_with(
            vec![json!({
                "id": "50",
                "name": "Integration",
                "position": 1,
                "permissions": "0",
                "managed": true,
            })],
            &[member_json(1000, &[10])],
            100,
        );
        let managed = role(&guild, 50);

        let err = validate_role_change(&guild, &managed, UserId::new(2000), UserId::new(1000))
            .expect_err("managed role is rejected");
        assert!(err.to_string().contains("managed"));
    }

    #[test]
    fn rejects_the_guild_owner_as_a_target() {
        let guild = standard_guild();
        let muted = role(&guild, 40);

        let err = validate_role_change(&guild, &muted, UserId::new(100), UserId::new(1000))
            .expect_err("the owner is protected");
        assert!(err.to_string().contains("owner"));
    }

    #[test]
    fn rejects_the_bot_as_a_target() {
        let guild = standard_guild();
        let muted = role(&guild, 40);

        let err = validate_role_change(&guild, &muted, UserId::new(1000), UserId::new(1000))
            .expect_err("the bot cannot edit itself");
        assert!(err.to_string().contains("own roles"));
    }

    #[test]
    fn rejects_a_role_at_or_above_the_bots_highest_role() {
        let guild = standard_guild();
        // The bot's highest role is Helper (position 3).
        let helper = role(&guild, 30);
        let admin = role(&guild, 10);

        for blocked in [&helper, &admin] {
            let err = validate_role_change(&guild, blocked, UserId::new(2000), UserId::new(1000))
                .expect_err("role at or above the bot is rejected");
            assert!(err.to_string().contains("not lower than my highest role"));
        }
    }

    #[test]
    fn rejects_a_target_whose_highest_role_is_not_lower_than_the_bots() {
        // The bot's highest role is Helper (position 3); the target's Moderator
        // is position 4, so the target outranks the bot.
        let guild = guild_with(
            vec![
                role_json(20, "Moderator", 4),
                role_json(30, "Helper", 3),
                role_json(40, "Muted", 1),
            ],
            &[member_json(1000, &[30]), member_json(3000, &[20])],
            100,
        );
        let muted = role(&guild, 40);

        let err = validate_role_change(&guild, &muted, UserId::new(3000), UserId::new(1000))
            .expect_err("the target outranks the bot");
        assert!(err.to_string().contains("That member's highest role"));
    }

    #[test]
    fn allows_a_lower_role_and_a_target_below_the_bot() {
        let guild = standard_guild();
        let muted = role(&guild, 40);

        assert!(validate_role_change(&guild, &muted, UserId::new(2000), UserId::new(1000)).is_ok());
    }

    #[test]
    fn allows_when_the_bots_member_object_is_not_cached() {
        // Without the bot's member object the hierarchy is unknown; Discord is
        // the authority, so this must not fail closed and block every change.
        let guild = guild_with(
            vec![role_json(10, "Admin", 5)],
            &[member_json(2000, &[])],
            100,
        );
        let admin = role(&guild, 10);

        assert!(validate_role_change(&guild, &admin, UserId::new(2000), UserId::new(9999)).is_ok());
    }

    #[test]
    fn allows_when_the_target_member_object_is_not_cached() {
        // The role is below the bot's highest role, so the target's own ranking
        // is the only unknown; Discord enforces it server-side.
        let guild = standard_guild();
        let muted = role(&guild, 40);

        assert!(validate_role_change(&guild, &muted, UserId::new(7777), UserId::new(1000)).is_ok());
    }

    // --- embeds -----------------------------------------------------------

    #[test]
    fn result_embed_reports_the_action_and_target() {
        let embed = result_embed(RoleAction::Add, "Muted", UserId::new(2000));

        assert_eq!(embed.title.as_deref(), Some("Role added"));
        assert_eq!(embed.color, Some(COLOR_ADDED));
        let description = embed.description.expect("the embed has a description");
        assert!(description.contains("Added"));
        assert!(description.contains("**Muted**"));
        assert!(description.contains("<@2000>"));
    }

    #[test]
    fn undone_embed_says_the_change_was_reverted() {
        let embed = undone_embed(RoleAction::Remove, "Muted", UserId::new(2000));

        assert_eq!(embed.title.as_deref(), Some("Role change undone"));
        assert_eq!(embed.color, Some(COLOR_UNDONE));
        let description = embed.description.expect("the embed has a description");
        assert!(description.contains("Reverted"));
        assert!(description.contains("**Muted**"));
    }

    #[test]
    fn undo_row_has_a_single_secondary_button_with_the_encoded_id() {
        let json = undo_row(undo(2000, 40, RoleAction::Add)).to_json();

        assert_eq!(json["type"], json!(1));
        assert_eq!(json["components"].as_array().map(Vec::len), Some(1));
        assert_eq!(json["components"][0]["type"], json!(2));
        assert_eq!(json["components"][0]["style"], json!(2));
        assert_eq!(json["components"][0]["label"], json!("Undo"));
        assert_eq!(
            json["components"][0]["custom_id"],
            json!("role-undo:2000:40:add")
        );
    }

    #[test]
    fn registration_uses_string_with_autocomplete_for_the_role_option() {
        let json = register().to_json();
        let options = json["options"].as_array().expect("the command has options");

        let role = options
            .iter()
            .find(|option| option["name"] == json!("role"))
            .expect("the role option is registered");
        assert_eq!(
            role["type"],
            json!(3),
            "autocomplete requires a string option"
        );
        assert_eq!(role["autocomplete"], json!(true));
        assert_eq!(role["required"], json!(true));

        let execute = options
            .iter()
            .find(|option| option["name"] == json!("execute"))
            .expect("the execute option is registered");
        assert_eq!(
            execute["choices"],
            json!([
                {"name": "Add", "value": "add"},
                {"name": "Remove", "value": "remove"}
            ]),
            "execute only offers the documented add/remove choices"
        );
    }
}
