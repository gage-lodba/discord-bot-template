use discord::client::Context;
use discord::id::GuildId;
use discord::model::user::User;

use tracing::info;

/// Notification-only counterpart to `guild_audit_log_entry_create.rs`.
///
/// The audit-log handler owns the full record -- who issued the ban, the reason,
/// and the embed posted to the log channel -- and logs it at `warn!`. This
/// handler only guarantees a line in the log when the audit-log entry is
/// unavailable (for example when `VIEW_AUDIT_LOG` is missing), so it stays a
/// single `info!` line rather than repeating the audit detail.
///
/// Reference: `docs/gateway-events.md` (Guild Ban Add).
pub(crate) fn guild_ban_addition(_ctx: Context, guild_id: GuildId, banned_user: &User) {
    info!(
        "{}[{}] was banned from guild {}",
        banned_user.username, banned_user.id, guild_id
    );
}
