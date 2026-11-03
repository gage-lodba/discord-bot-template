use discord::builder::embed::EmbedBuilder;
use discord::builder::message::MessageBuilder;
use discord::client::Context;
use discord::id::GenericId;
use discord::id::GuildId;
use discord::id::UserId;
use discord::model::audit_log::AuditLogAction;
use discord::model::audit_log::AuditLogEntry;
use discord::timestamp::Timestamp;

use tracing::error;
use tracing::warn;

use crate::config::Ids;
use crate::moderation;
use crate::moderation::Moderation;

/// Renders an entry's target for embeds and log lines.
///
/// `target_id` is optional in Discord's schema (`docs/audit-log.md`, Audit Log
/// Entry Object: `target_id | ?string`), so an absent target degrades to a
/// readable placeholder instead of panicking or dropping the whole entry.
fn target_or_unknown(target_id: Option<GenericId>) -> String {
    target_id.map_or_else(|| "unknown".to_owned(), |id| id.to_string())
}

/// Converts an audit entry target into a user id, if it names one.
fn target_user(target_id: Option<GenericId>) -> Option<UserId> {
    target_id.map(|id| UserId::new(id.get()))
}

/// Formats the actor of an entry as a mention.
///
/// `user_id` is nullable in the schema (entries caused by Discord itself have no
/// actor), so it degrades to a placeholder rather than panicking.
fn moderator(entry: &AuditLogEntry) -> String {
    entry
        .user_id
        .map_or_else(|| "unknown".to_owned(), |id| format!("<@{id}>"))
}

// Division of labour with `guild_ban_addition.rs` / `guild_ban_removal.rs`: this
// handler owns the audit trail -- who acted, on whom, and why -- and logs it at
// `warn!`; the ban event handlers are plain `info!` notifications. Do not repeat
// the audit detail there, or the same ban is logged twice at the same level.
//
// This handler is also the only place that knows a removal was moderation rather
// than a voluntary leave, so it records kick and ban targets in `moderation` for
// `guild_member_removal` to consult. The record is taken before the embed is
// sent, so a slow or failing send cannot cause a duplicate announcement.
//
// Reference: `docs/audit-log.md` (Audit Log Entry Object, Audit Log Events) and
// `docs/gateway-events.md` (Guild Audit Log Entry Create).
pub(crate) async fn guild_audit_log_entry_create(
    ctx: Context,
    entry: AuditLogEntry,
    guild_id: GuildId,
    ids: Ids,
) {
    let target = target_or_unknown(entry.target_id);
    let target_user = target_user(entry.target_id);
    let reason = entry.reason.as_deref().unwrap_or_default();
    let moderator = moderator(&entry);
    let log_channel = ids.log_channel;

    match entry.action {
        AuditLogAction::MemberBanAdd => {
            if let Some(user_id) = target_user {
                moderation::record(user_id, Moderation::Ban);
            }

            let embed = EmbedBuilder::new()
                .description(format!(
                    "### 🔨 Ban\n\
                     Target: `{target}`\n\
                     Moderator: {moderator}\n\
                     Reason: ```\n{reason}```"
                ))
                .color(0x00f4_4336)
                .timestamp(Timestamp::now());

            let builder = MessageBuilder::new().embed(embed.build());
            if let Err(err) = log_channel.send_message(&ctx.http, &builder).await {
                error!(
                    "failed to post ban audit log in guild {} for target {}: {}",
                    guild_id, target, err
                );
                return;
            }

            warn!(
                "{} was banned by {} in guild {}. Reason: {}",
                target, moderator, guild_id, reason
            );
        }

        AuditLogAction::MemberBanRemove => {
            let embed = EmbedBuilder::new()
                .description(format!(
                    "### ✅ Unbanned\n\
                     Target: `{target}`\n\
                     Moderator: {moderator}"
                ))
                .color(0x004c_af50)
                .timestamp(Timestamp::now());

            let builder = MessageBuilder::new().embed(embed.build());
            if let Err(err) = log_channel.send_message(&ctx.http, &builder).await {
                error!(
                    "failed to post unban audit log in guild {} for target {}: {}",
                    guild_id, target, err
                );
                return;
            }

            warn!(
                "{} was unbanned by {} in guild {}.",
                target, moderator, guild_id
            );
        }

        AuditLogAction::MemberKick => {
            if let Some(user_id) = target_user {
                moderation::record(user_id, Moderation::Kick);
            }

            let embed = EmbedBuilder::new()
                .description(format!(
                    "### 👢 Kick\n\
                     Target: `{target}`\n\
                     Moderator: {moderator}\n\
                     Reason: ```\n{reason}```"
                ))
                .color(0x00f4_4336)
                .timestamp(Timestamp::now());

            let builder = MessageBuilder::new().embed(embed.build());
            if let Err(err) = log_channel.send_message(&ctx.http, &builder).await {
                error!(
                    "failed to post kick audit log in guild {} for target {}: {}",
                    guild_id, target, err
                );
                return;
            }

            warn!(
                "{} was kicked by {} in guild {}. Reason: {}",
                target, moderator, guild_id, reason
            );
        }

        _ => (),
    }
}
