//! Dispatch of received gateway events.
//!
//! A dispatch (op 0) payload carries an event name and an event body. Handling is
//! split in two: the cache is updated synchronously (so the bot's reads see the
//! new state before the handler runs), then the matching [`EventHandler`] method
//! is invoked in a spawned task. Handlers may take as long as they like without
//! stalling the websocket read loop, and a malformed event logs and is skipped
//! rather than closing the connection.
//!
//! Reference: `docs/gateway-events.md` (Payload Structure, Receive Events).

use std::future::Future;
use std::sync::Arc;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::builder::interaction::InteractionResponse;
use crate::client::{Context, EventHandler};
use crate::error::Error;
use crate::id::GuildId;
use crate::model::application::Interaction;
use crate::model::audit_log::AuditLogEntry;
use crate::model::guild::Member;
use crate::model::message::Message;
use crate::model::ready::{Ready, Resumed};
use crate::model::user::User;

/// The handler an incoming dispatch event is routed to.
///
/// Every other event is [`EventRoute::Unknown`]: the client logs it at debug
/// level and ignores it, which keeps the crate working when Discord adds events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventRoute {
    /// The initial state after a successful Identify (`on_ready`).
    Ready,
    /// A successful Resume (`on_resumed`).
    Resumed,
    /// A guild became available (`GUILD_CREATE`).
    GuildCreate,
    /// A user joined a guild (`GUILD_MEMBER_ADD`).
    GuildMemberAdd,
    /// A user left/was removed from a guild (`GUILD_MEMBER_REMOVE`).
    GuildMemberRemove,
    /// A user was banned (`GUILD_BAN_ADD`).
    GuildBanAdd,
    /// A user was unbanned (`GUILD_BAN_REMOVE`).
    GuildBanRemove,
    /// An audit log entry was created (`GUILD_AUDIT_LOG_ENTRY_CREATE`).
    GuildAuditLogEntryCreate,
    /// A message was created (`MESSAGE_CREATE`).
    MessageCreate,
    /// An interaction was used (`INTERACTION_CREATE`).
    InteractionCreate,
    /// An event the client does not handle.
    Unknown,
}

/// Maps a dispatch event name to the handler route.
///
/// Reference: `docs/gateway-events.md` (Event Names, Receive Events).
#[must_use]
pub fn route(event_name: &str) -> EventRoute {
    match event_name {
        "READY" => EventRoute::Ready,
        "RESUMED" => EventRoute::Resumed,
        "GUILD_CREATE" => EventRoute::GuildCreate,
        "GUILD_MEMBER_ADD" => EventRoute::GuildMemberAdd,
        "GUILD_MEMBER_REMOVE" => EventRoute::GuildMemberRemove,
        "GUILD_BAN_ADD" => EventRoute::GuildBanAdd,
        "GUILD_BAN_REMOVE" => EventRoute::GuildBanRemove,
        "GUILD_AUDIT_LOG_ENTRY_CREATE" => EventRoute::GuildAuditLogEntryCreate,
        "MESSAGE_CREATE" => EventRoute::MessageCreate,
        "INTERACTION_CREATE" => EventRoute::InteractionCreate,
        _ => EventRoute::Unknown,
    }
}

/// Whether an inbound interaction must be acknowledged automatically before it
/// reaches the handler.
///
/// Reference: `docs/interactions-overview.md` (Acknowledging PING requests) and
/// `docs/receiving-and-responding.md` (Interaction Callback Type, `PONG`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InteractionAck {
    /// A `PING` interaction, which Discord requires to be acknowledged with a
    /// `PONG` callback within 3 seconds.
    Pong,
    /// An interaction another handler response will acknowledge.
    None,
}

/// Decides whether one parsed interaction needs the automatic `PONG`
/// acknowledgement.
///
/// Split from the dispatch itself so the decision is unit-testable without an
/// HTTP server; the HTTP call itself is covered by the HTTP client tests.
pub(crate) const fn interaction_ack(interaction: &Interaction) -> InteractionAck {
    match interaction {
        Interaction::Ping(_) => InteractionAck::Pong,
        Interaction::Command(_)
        | Interaction::Component(_)
        | Interaction::Autocomplete(_)
        | Interaction::ModalSubmit(_)
        | Interaction::Unknown(_) => InteractionAck::None,
    }
}

/// The event body of Guild Ban Add/Remove and Guild Member Remove: a user plus
/// the id of the guild it happened in.
///
/// Reference: `docs/gateway-events.md` (Guild Ban Add, Guild Ban Remove, Guild
/// Member Remove).
#[derive(Debug, Deserialize)]
struct GuildUserEvent {
    guild_id: GuildId,
    user: User,
}

/// Updates the cache and invokes the handler for one dispatch payload.
///
/// The cache is updated before the handler task is spawned. Handler panics stay
/// inside the spawned task and cannot take the connection down.
///
/// # Errors
///
/// Returns [`Error::Json`] when a payload the client handles does not match the
/// documented shape; the caller logs it and keeps the connection open.
pub(crate) fn handle<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    event_name: &str,
    data: Value,
) -> Result<(), Error> {
    match route(event_name) {
        EventRoute::Ready => handle_ready(ctx, handler, &data),
        EventRoute::Resumed => {
            handle_resumed(ctx, handler);
            Ok(())
        }
        EventRoute::GuildCreate => handle_guild_create(ctx, data),
        EventRoute::GuildMemberAdd => handle_guild_member_add(ctx, handler, data),
        EventRoute::GuildMemberRemove => handle_guild_member_remove(ctx, handler, data),
        EventRoute::GuildBanAdd => handle_guild_ban_add(ctx, handler, data),
        EventRoute::GuildBanRemove => handle_guild_ban_remove(ctx, handler, data),
        EventRoute::GuildAuditLogEntryCreate => handle_guild_audit_log_entry(ctx, handler, data),
        EventRoute::MessageCreate => handle_message_create(ctx, handler, data),
        EventRoute::InteractionCreate => handle_interaction_create(ctx, handler, data),
        EventRoute::Unknown => {
            tracing::debug!(event = event_name, "ignoring an unhandled gateway event");
            Ok(())
        }
    }
}

/// Handles `READY`: caches the bot user, the unavailable guilds and the
/// application id, then delivers the full payload to `on_ready`.
///
/// Reference: `docs/gateway-events.md` (Ready Event).
fn handle_ready<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: &Value,
) -> Result<(), Error> {
    let ready: Ready = serde_json::from_value(data.clone())?;
    // The cached user is parsed separately so the handler keeps the full
    // Ready body; this avoids relying on `User: Clone`.
    let user: User = parse_field(data, "user")?;

    ctx.cache.set_current_user(user);
    ctx.cache
        .set_unavailable_guilds(ready.guilds.iter().map(|guild| guild.id));
    ctx.cache.set_application_id(ready.application.id);
    ctx.http.set_application_id(ready.application.id);

    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler.on_ready(ctx, ready).await;
    });

    Ok(())
}

/// Handles `RESUMED` by delivering it to `on_resumed`.
///
/// Reference: `docs/gateway-events.md` (Resumed).
fn handle_resumed<H: EventHandler>(ctx: &Context, handler: &Arc<H>) {
    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler.on_resumed(ctx, Resumed).await;
    });
}

/// Handles `GUILD_CREATE`: caches the guild, or records the unavailable-guild
/// placeholder and waits for the real event.
///
/// Reference: `docs/gateway-events.md` (Guild Create).
fn handle_guild_create(ctx: &Context, data: Value) -> Result<(), Error> {
    if is_unavailable_guild(&data) {
        if let Ok(guild_id) = parse_field::<GuildId>(&data, "id") {
            ctx.cache.mark_guild_unavailable(guild_id);
        }
        tracing::debug!("a guild is unavailable; waiting for its Guild Create");

        return Ok(());
    }

    let guild: crate::model::guild::Guild = serde_json::from_value(data)?;
    ctx.cache.insert_guild(guild);

    Ok(())
}

/// Handles `GUILD_MEMBER_ADD`: caches the member with the envelope's guild id and
/// passes a copy to `on_guild_member_add`.
///
/// Reference: `docs/gateway-events.md` (Guild Member Add, Extra Fields).
fn handle_guild_member_add<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: Value,
) -> Result<(), Error> {
    let guild_id = guild_id_field(&data)?;

    // The cache keeps the member with the guild id filled in from the
    // event envelope; the handler receives its own copy.
    let mut cached: Member = serde_json::from_value(data.clone())?;
    cached.guild_id = Some(guild_id);
    ctx.cache.update_member(guild_id, cached);

    let mut member: Member = serde_json::from_value(data)?;
    member.guild_id = Some(guild_id);

    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler.on_guild_member_add(ctx, member).await;
    });

    Ok(())
}

/// Handles `GUILD_MEMBER_REMOVE`: drops the cached member (keeping a copy for
/// the handler) and passes the user to `on_guild_member_remove`.
///
/// Reference: `docs/gateway-events.md` (Guild Member Remove).
fn handle_guild_member_remove<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: Value,
) -> Result<(), Error> {
    let event: GuildUserEvent = serde_json::from_value(data)?;

    // Keep the cached copy, if any: the event itself only carries the
    // user, but handlers often want the member's nick and roles.
    let cached_member = ctx
        .cache
        .guild(event.guild_id)
        .and_then(|guild| guild.members.get(&event.user.id).cloned());
    ctx.cache.remove_member(event.guild_id, event.user.id);

    let GuildUserEvent { guild_id, user } = event;
    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler
            .on_guild_member_remove(ctx, guild_id, user, cached_member)
            .await;
    });

    Ok(())
}

/// Handles `GUILD_BAN_ADD` by passing the user and guild id to
/// `on_guild_ban_add`.
///
/// Reference: `docs/gateway-events.md` (Guild Ban Add).
fn handle_guild_ban_add<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: Value,
) -> Result<(), Error> {
    let GuildUserEvent { guild_id, user } = serde_json::from_value(data)?;

    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler.on_guild_ban_add(ctx, guild_id, user).await;
    });

    Ok(())
}

/// Handles `GUILD_BAN_REMOVE` by passing the user and guild id to
/// `on_guild_ban_remove`.
///
/// Reference: `docs/gateway-events.md` (Guild Ban Remove).
fn handle_guild_ban_remove<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: Value,
) -> Result<(), Error> {
    let GuildUserEvent { guild_id, user } = serde_json::from_value(data)?;

    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler.on_guild_ban_remove(ctx, guild_id, user).await;
    });

    Ok(())
}

/// Handles `GUILD_AUDIT_LOG_ENTRY_CREATE`: passes the entry and its guild id to
/// `on_guild_audit_log_entry_create`; an entry without a guild id is logged and
/// skipped.
///
/// Reference: `docs/gateway-events.md` (Guild Audit Log Entry Create).
fn handle_guild_audit_log_entry<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: Value,
) -> Result<(), Error> {
    let entry: AuditLogEntry = serde_json::from_value(data)?;

    let Some(guild_id) = entry.guild_id else {
        tracing::warn!("ignoring an audit log entry without a guild id");

        return Ok(());
    };

    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler
            .on_guild_audit_log_entry_create(ctx, entry, guild_id)
            .await;
    });

    Ok(())
}

/// Handles `MESSAGE_CREATE` by passing the message to `on_message_create`.
///
/// Reference: `docs/gateway-events.md` (Message Create).
fn handle_message_create<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: Value,
) -> Result<(), Error> {
    let message: Message = serde_json::from_value(data)?;

    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler.on_message_create(ctx, message).await;
    });

    Ok(())
}

/// Handles `INTERACTION_CREATE`, first auto-acknowledging a `PING` interaction
/// with the documented `PONG` callback and then delivering the interaction to
/// `on_interaction_create`.
///
/// Reference: `docs/interactions-overview.md` (Acknowledging PING requests) and
/// `docs/receiving-and-responding.md` (Interaction Callback Type, `PONG`).
fn handle_interaction_create<H: EventHandler>(
    ctx: &Context,
    handler: &Arc<H>,
    data: Value,
) -> Result<(), Error> {
    let interaction: Interaction = serde_json::from_value(data)?;

    // Discord requires a PING interaction to be acknowledged with a
    // PONG callback within 3 seconds, before any handler runs
    // (docs/interactions-overview.md, Acknowledging PING requests).
    if interaction_ack(&interaction) == InteractionAck::Pong
        && let Interaction::Ping(ping) = &interaction
    {
        let http = ctx.http.clone();
        let interaction_id = ping.id;
        let token = ping.token.clone();

        tokio::spawn(async move {
            if let Err(error) = http
                .create_interaction_response(interaction_id, &token, InteractionResponse::Pong)
                .await
            {
                tracing::warn!(
                    %error,
                    "failed to acknowledge a PING interaction with a PONG"
                );
            }
        });
    }

    // The interaction is still delivered to the handler afterwards,
    // best-effort: a failed PONG is logged above and never blocks the
    // handler.
    spawn_task(handler, ctx, move |handler, ctx| async move {
        handler.on_interaction_create(ctx, interaction).await;
    });

    Ok(())
}

/// Spawns one handler call so a slow handler cannot stall the websocket loop.
fn spawn_task<H, F, Fut>(handler: &Arc<H>, ctx: &Context, task: F)
where
    H: EventHandler,
    F: FnOnce(Arc<H>, Context) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let handler = Arc::clone(handler);
    let ctx = ctx.clone();

    tokio::spawn(async move {
        task(handler, ctx).await;
    });
}

/// Returns true when a Guild Create body is the unavailable-guild form.
///
/// An unavailable guild is `{ "id": ..., "unavailable": true }`; an available
/// guild always carries at least its name.
///
/// Reference: `docs/gateway-events.md` (Guild Create).
fn is_unavailable_guild(data: &Value) -> bool {
    let explicitly_unavailable = data
        .get("unavailable")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    explicitly_unavailable || (data.get("id").is_some() && data.get("name").is_none())
}

/// Reads the `guild_id` extra field of an event envelope.
///
/// Reference: `docs/gateway-events.md` (Guild Member Add Extra Fields,
/// Guild Audit Log Entry Create Event Extra Fields).
fn guild_id_field(data: &Value) -> Result<GuildId, Error> {
    parse_field(data, "guild_id")
}

/// Deserialises one field of an event body.
fn parse_field<T: DeserializeOwned>(data: &Value, field: &str) -> Result<T, Error> {
    let value = data.get(field).cloned().unwrap_or(Value::Null);

    serde_json::from_value(value).map_err(Error::from)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use serde_json::json;

    use super::*;
    use crate::cache::Cache;
    use crate::client::{Data, ShardHandle};
    use crate::http::client::HttpClient;

    struct TestHandler;

    impl EventHandler for TestHandler {}

    fn test_context() -> Context {
        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();

        Context::new(
            HttpClient::new("test-token"),
            Arc::new(Cache::new()),
            Arc::new(tokio::sync::RwLock::new(Data::default())),
            ShardHandle::new(out_tx, Arc::new(AtomicBool::new(false))),
        )
    }

    fn test_handler() -> Arc<TestHandler> {
        Arc::new(TestHandler)
    }

    fn ready_payload() -> Value {
        json!({
            "v": 10,
            "user": {
                "id": "2",
                "username": "example-user",
                "discriminator": "0",
                "global_name": null,
                "avatar": null,
                "bot": true,
                "system": false,
            },
            "guilds": [
                {"id": "10", "unavailable": true},
                {"id": "11", "unavailable": true},
            ],
            "session_id": "session",
            "resume_gateway_url": "wss://gateway-us-east1-b.discord.gg",
            "application": {"id": "3", "flags": 0},
        })
    }

    fn guild_payload() -> Value {
        json!({
            "id": "10",
            "name": "Nocturne",
            "icon": null,
            "owner_id": "1",
            "roles": [],
            "channels": [],
            "threads": [],
            "members": [],
            "features": [],
            "member_count": 2,
        })
    }

    fn member_payload() -> Value {
        json!({
            "guild_id": "10",
            "user": {
                "id": "20",
                "username": "member",
                "discriminator": "0",
                "bot": false,
                "system": false,
            },
            "nick": null,
            "roles": [],
            "joined_at": "2026-10-22T13:20:32.123+00:00",
            "deaf": false,
            "mute": false,
            "pending": null,
        })
    }

    #[test]
    fn documented_events_route_to_their_handlers() {
        let cases = [
            ("READY", EventRoute::Ready),
            ("RESUMED", EventRoute::Resumed),
            ("GUILD_CREATE", EventRoute::GuildCreate),
            ("GUILD_MEMBER_ADD", EventRoute::GuildMemberAdd),
            ("GUILD_MEMBER_REMOVE", EventRoute::GuildMemberRemove),
            ("GUILD_BAN_ADD", EventRoute::GuildBanAdd),
            ("GUILD_BAN_REMOVE", EventRoute::GuildBanRemove),
            (
                "GUILD_AUDIT_LOG_ENTRY_CREATE",
                EventRoute::GuildAuditLogEntryCreate,
            ),
            ("MESSAGE_CREATE", EventRoute::MessageCreate),
            ("INTERACTION_CREATE", EventRoute::InteractionCreate),
        ];

        for (name, expected) in cases {
            assert_eq!(route(name), expected, "event {name}");
        }

        assert_eq!(route("TYPING_START"), EventRoute::Unknown);
        assert_eq!(route(""), EventRoute::Unknown);
    }

    #[tokio::test]
    async fn ready_updates_the_cache_and_the_http_application_id() {
        let ctx = test_context();
        let handler = test_handler();

        handle(&ctx, &handler, "READY", ready_payload()).expect("ready is handled");

        assert_eq!(ctx.cache.current_user().map(|user| user.id.get()), Some(2));
        assert_eq!(ctx.cache.application_id().map(crate::id::Id::get), Some(3));
        assert_eq!(ctx.http.application_id().map(crate::id::Id::get), Some(3));
        assert!(ctx.cache.is_guild_unavailable(GuildId::new(10)));
        assert!(ctx.cache.is_guild_unavailable(GuildId::new(11)));
        assert!(ctx.cache.guilds().is_empty());
    }

    #[tokio::test]
    async fn guild_create_caches_an_available_guild() {
        let ctx = test_context();
        let handler = test_handler();

        handle(&ctx, &handler, "GUILD_CREATE", guild_payload()).expect("guild create is handled");

        let guild = ctx.cache.guild(GuildId::new(10)).expect("guild is cached");
        assert_eq!(guild.name, "Nocturne");
        assert!(!ctx.cache.is_guild_unavailable(GuildId::new(10)));
    }

    #[tokio::test]
    async fn unavailable_guild_create_is_not_cached_as_a_guild() {
        let ctx = test_context();
        let handler = test_handler();

        handle(
            &ctx,
            &handler,
            "GUILD_CREATE",
            json!({"id": "10", "unavailable": true}),
        )
        .expect("unavailable guild create is handled");

        assert!(ctx.cache.guild(GuildId::new(10)).is_none());
    }

    #[tokio::test]
    async fn guild_member_add_fills_the_guild_id_and_caches_the_member() {
        let ctx = test_context();
        let handler = std::sync::Arc::new(GuildEventRecorder::new());
        handle(&ctx, &handler, "GUILD_CREATE", guild_payload()).expect("guild create is handled");

        handle(&ctx, &handler, "GUILD_MEMBER_ADD", member_payload())
            .expect("member add is handled");

        let guild = ctx.cache.guild(GuildId::new(10)).expect("guild is cached");
        let member = guild
            .members
            .get(&crate::id::UserId::new(20))
            .expect("member is cached");

        assert_eq!(member.guild_id, Some(GuildId::new(10)));

        // The handler is spawned after the cache update, so yield first.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(handler.recorded(), ["member_add:20"]);
    }

    #[tokio::test]
    async fn guild_member_remove_drops_the_cached_member() {
        let ctx = test_context();
        let handler = test_handler();
        handle(&ctx, &handler, "GUILD_CREATE", guild_payload()).expect("guild create is handled");
        handle(&ctx, &handler, "GUILD_MEMBER_ADD", member_payload())
            .expect("member add is handled");

        handle(
            &ctx,
            &handler,
            "GUILD_MEMBER_REMOVE",
            json!({"guild_id": "10", "user": {"id": "20", "username": "member", "discriminator": "0", "bot": false, "system": false}}),
        )
        .expect("member remove is handled");

        let guild = ctx.cache.guild(GuildId::new(10)).expect("guild is cached");
        assert!(!guild.members.contains_key(&crate::id::UserId::new(20)));
    }

    #[tokio::test]
    async fn malformed_payloads_return_an_error_instead_of_panicking() {
        let ctx = test_context();
        let handler = test_handler();

        let error = handle(
            &ctx,
            &handler,
            "MESSAGE_CREATE",
            json!({"id": "not-a-snowflake"}),
        )
        .expect_err("malformed message is an error");

        assert!(matches!(error, Error::Json(_)));
    }

    #[tokio::test]
    async fn unknown_events_are_ignored() {
        let ctx = test_context();
        let handler = test_handler();

        handle(&ctx, &handler, "TYPING_START", json!({"channel_id": "1"}))
            .expect("unknown events are ignored");
    }

    fn command_interaction_payload() -> Value {
        json!({
            "type": 2,
            "id": "100",
            "application_id": "200",
            "token": "interaction-token",
            "guild_id": "10",
            "channel_id": "20",
            "user": {"id": "500", "username": "invoker"},
            "data": {"id": "700", "name": "ping", "type": 1}
        })
    }

    #[test]
    fn a_ping_interaction_is_parsed_and_needs_a_pong() {
        let interaction: Interaction = serde_json::from_value(json!({
            "type": 1,
            "id": "100",
            "application_id": "200",
            "token": "interaction-token",
            "version": 1
        }))
        .expect("ping payload parses");

        assert_eq!(
            interaction.kind(),
            crate::model::application::InteractionType::Ping
        );
        assert_eq!(interaction_ack(&interaction), InteractionAck::Pong);

        let Interaction::Ping(ping) = &interaction else {
            panic!("expected a typed ping interaction");
        };
        assert_eq!(ping.id, crate::id::InteractionId::new(100));
        assert_eq!(ping.application_id, crate::id::ApplicationId::new(200));
        assert_eq!(ping.token, "interaction-token");
        assert_eq!(ping.version, 1);
    }

    #[test]
    fn only_ping_interactions_are_acknowledged_automatically() {
        let command: Interaction =
            serde_json::from_value(command_interaction_payload()).expect("command payload parses");
        assert_eq!(interaction_ack(&command), InteractionAck::None);

        for payload in [
            json!({
                "type": 3,
                "id": "1",
                "application_id": "2",
                "token": "t",
                "user": {"id": "3", "username": "u"},
                "data": {"custom_id": "c", "component_type": 2}
            }),
            json!({
                "type": 4,
                "id": "1",
                "application_id": "2",
                "token": "t",
                "user": {"id": "3", "username": "u"},
                "data": {"id": "4", "name": "n", "type": 1, "options": []}
            }),
            json!({
                "type": 5,
                "id": "1",
                "application_id": "2",
                "token": "t",
                "user": {"id": "3", "username": "u"},
                "data": {"custom_id": "c", "components": []}
            }),
            json!({"type": 99}),
        ] {
            let interaction: Interaction =
                serde_json::from_value(payload).expect("interaction payload parses");
            assert_eq!(interaction_ack(&interaction), InteractionAck::None);
        }
    }

    #[tokio::test]
    async fn interaction_create_still_delivers_every_interaction_to_the_handler() {
        let ctx = test_context();
        let recorded = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let handler = std::sync::Arc::new(RecordingHandler {
            interactions: std::sync::Arc::clone(&recorded),
        });

        handle(
            &ctx,
            &handler,
            "INTERACTION_CREATE",
            command_interaction_payload(),
        )
        .expect("interaction create is handled");

        // The handler runs in a spawned task so it cannot stall the websocket
        // loop; yield so the task can run.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let recorded = recorded.lock().expect("interaction log is not poisoned");
        assert_eq!(recorded.len(), 1);
        assert_eq!(
            recorded[0].kind(),
            crate::model::application::InteractionType::ApplicationCommand
        );
    }

    #[test]
    fn a_malformed_interaction_payload_is_an_error() {
        let ctx = test_context();
        let handler = test_handler();

        let error = handle(
            &ctx,
            &handler,
            "INTERACTION_CREATE",
            json!({"type": 2, "id": "1", "application_id": "2", "token": "t"}),
        )
        .expect_err("a command interaction without data is malformed");

        assert!(matches!(error, Error::Json(_)));
    }

    /// An [`EventHandler`] that records the guild events it receives.
    struct GuildEventRecorder {
        events: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl GuildEventRecorder {
        fn new() -> Self {
            Self {
                events: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn recorded(&self) -> Vec<String> {
            self.events
                .lock()
                .expect("the event log is not poisoned")
                .clone()
        }
    }

    impl EventHandler for GuildEventRecorder {
        fn on_guild_member_add(
            &self,
            _ctx: Context,
            member: Member,
        ) -> impl Future<Output = ()> + Send {
            let events = std::sync::Arc::clone(&self.events);

            async move {
                events
                    .lock()
                    .expect("the event log is not poisoned")
                    .push(format!("member_add:{}", member.user.id.get()));
            }
        }

        fn on_guild_ban_add(
            &self,
            _ctx: Context,
            guild_id: GuildId,
            user: User,
        ) -> impl Future<Output = ()> + Send {
            let events = std::sync::Arc::clone(&self.events);

            async move {
                events
                    .lock()
                    .expect("the event log is not poisoned")
                    .push(format!("ban_add:{}:{}", guild_id.get(), user.id.get()));
            }
        }

        fn on_guild_ban_remove(
            &self,
            _ctx: Context,
            guild_id: GuildId,
            user: User,
        ) -> impl Future<Output = ()> + Send {
            let events = std::sync::Arc::clone(&self.events);

            async move {
                events
                    .lock()
                    .expect("the event log is not poisoned")
                    .push(format!("ban_remove:{}:{}", guild_id.get(), user.id.get()));
            }
        }

        fn on_guild_audit_log_entry_create(
            &self,
            _ctx: Context,
            entry: AuditLogEntry,
            guild_id: GuildId,
        ) -> impl Future<Output = ()> + Send {
            let events = std::sync::Arc::clone(&self.events);

            async move {
                events
                    .lock()
                    .expect("the event log is not poisoned")
                    .push(format!(
                        "audit_log:{}:{}:{}",
                        guild_id.get(),
                        entry.id.get(),
                        entry.action.as_i64()
                    ));
            }
        }
    }

    /// The documented body of a `GUILD_AUDIT_LOG_ENTRY_CREATE` dispatch: an
    /// audit log entry plus the extra `guild_id` field
    /// (`docs/gateway-events.md`, Guild Audit Log Entry Create Event Extra
    /// Fields).
    fn audit_log_entry_payload() -> Value {
        json!({
            "id": "610036311282647040",
            "action_type": 22,
            "user_id": "53908099506183680",
            "target_id": "245538417976458753",
            "reason": "mentioning b1nzy",
            "changes": [],
            "options": {"channel_id": "290926798626357248"},
            "guild_id": "290926798626357250",
        })
    }

    #[tokio::test]
    async fn documented_ban_events_are_parsed_and_dispatched() {
        // docs/gateway-events.md (Guild Ban Add / Guild Ban Remove Event
        // Fields): `guild_id` plus a `user` object.
        let ctx = test_context();
        let recorder = std::sync::Arc::new(GuildEventRecorder::new());

        handle(
            &ctx,
            &recorder,
            "GUILD_BAN_ADD",
            json!({"guild_id": "10", "user": {"id": "20", "username": "banned"}}),
        )
        .expect("a documented ban add is handled");
        handle(
            &ctx,
            &recorder,
            "GUILD_BAN_REMOVE",
            json!({"guild_id": "11", "user": {"id": "21", "username": "unbanned"}}),
        )
        .expect("a documented ban remove is handled");

        // Handlers run in spawned tasks, so yield before reading the log.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        assert_eq!(recorder.recorded(), ["ban_add:10:20", "ban_remove:11:21"]);
    }

    #[tokio::test]
    async fn guild_audit_log_entry_create_reads_the_extra_guild_id() {
        let ctx = test_context();
        let recorder = std::sync::Arc::new(GuildEventRecorder::new());

        handle(
            &ctx,
            &recorder,
            "GUILD_AUDIT_LOG_ENTRY_CREATE",
            audit_log_entry_payload(),
        )
        .expect("the documented audit log entry is handled");

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        assert_eq!(
            recorder.recorded(),
            ["audit_log:290926798626357250:610036311282647040:22"]
        );
    }

    #[tokio::test]
    async fn audit_log_entry_without_the_extra_guild_id_is_ignored() {
        let ctx = test_context();
        let recorder = std::sync::Arc::new(GuildEventRecorder::new());

        let mut payload = audit_log_entry_payload();
        payload
            .as_object_mut()
            .expect("the payload is an object")
            .remove("guild_id");

        handle(&ctx, &recorder, "GUILD_AUDIT_LOG_ENTRY_CREATE", payload)
            .expect("an entry without a guild id is not an error");

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        assert!(recorder.recorded().is_empty());
    }

    /// An [`EventHandler`] that records the interactions it receives.
    struct RecordingHandler {
        interactions: std::sync::Arc<std::sync::Mutex<Vec<Interaction>>>,
    }

    impl EventHandler for RecordingHandler {
        fn on_interaction_create(
            &self,
            _ctx: Context,
            interaction: Interaction,
        ) -> impl Future<Output = ()> + Send {
            let interactions = std::sync::Arc::clone(&self.interactions);

            async move {
                interactions
                    .lock()
                    .expect("the interaction log is not poisoned")
                    .push(interaction);
            }
        }
    }
}
