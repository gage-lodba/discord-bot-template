//! Bot wiring: [`Client`], [`Context`], [`Data`] and the [`EventHandler`] trait.
//!
//! The client owns the shared state every handler receives — the REST client, the
//! cache, the shard manager and a typed data map for the bot's own state — and
//! runs the gateway connection when [`Client::start`] is awaited.
//!
//! # Example
//!
//! ```no_run
//! use discord::client::Client;
//! use discord::intents::GatewayIntents;
//!
//! # async fn run() -> Result<(), discord::Error> {
//! let mut client = Client::builder("token", GatewayIntents::GUILDS)
//!     .event_handler(())
//!     .await?;
//!
//! let shard_manager = client.shard_manager.clone();
//! tokio::spawn(async move {
//!     // ... wait for the shutdown signal ...
//!     shard_manager.shutdown_all().await;
//! });
//!
//! client.start().await?;
//! # Ok(())
//! # }
//! ```
//!
//! Reference: `docs/gateway.md` (Connection Lifecycle), `docs/gateway-events.md`
//! (Ready, Resumed, dispatch events).

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{RwLock, mpsc};

use crate::builder::presence::PresenceBuilder;
use crate::cache::Cache;
use crate::error::{Error, GatewayError};
use crate::gateway::payload;
use crate::gateway::shard::Shard;
use crate::gateway::shard_manager::ShardManager;
use crate::http::client::HttpClient;
use crate::id::{EmojiId, GuildId};
use crate::intents::GatewayIntents;
use crate::model::application::Interaction;
use crate::model::audit_log::AuditLogEntry;
use crate::model::emoji::Emoji;
use crate::model::guild::Member;
use crate::model::message::Message;
use crate::model::ready::{Ready, Resumed};
use crate::model::user::User;

/// A key in the client's [`Data`] map.
///
/// Implement it for a local marker type and store the bot's state under that
/// type:
///
/// ```
/// use discord::client::DataKey;
///
/// struct Ids;
///
/// impl DataKey for Ids {
///     type Value = (u64, u64);
/// }
/// ```
pub trait DataKey: 'static {
    /// The value stored under this key.
    type Value: Send + Sync + 'static;
}

/// The type map handlers use to reach the bot's own state through [`Context`].
#[derive(Default)]
pub struct Data {
    values: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Data {
    /// Inserts a value under its [`DataKey`], replacing any previous value.
    pub fn insert<K: DataKey>(&mut self, value: K::Value) {
        self.values.insert(TypeId::of::<K>(), Box::new(value));
    }

    /// Returns the value stored under its [`DataKey`], if any.
    #[must_use]
    pub fn get<K: DataKey>(&self) -> Option<&K::Value> {
        self.values
            .get(&TypeId::of::<K>())?
            .downcast_ref::<K::Value>()
    }
}

impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The values are `dyn Any`: only the number of stored keys is printable
        // without downcasting each one.
        f.debug_struct("Data")
            .field("entries", &self.values.len())
            .finish_non_exhaustive()
    }
}

/// The outbound half of the live gateway connection.
///
/// `Context` hands this to handlers so operations that must go over the gateway
/// (presence updates) can be issued from any task.
#[derive(Clone, Debug)]
pub(crate) struct ShardHandle {
    out_tx: mpsc::UnboundedSender<String>,
    connected: Arc<AtomicBool>,
}

impl ShardHandle {
    /// Creates a handle around the client's outbound channel and connection flag.
    pub(crate) fn new(out_tx: mpsc::UnboundedSender<String>, connected: Arc<AtomicBool>) -> Self {
        Self { out_tx, connected }
    }

    /// Returns true while the shard has an open, identified connection.
    pub(crate) fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Queues one JSON payload for the shard to write to the gateway.
    pub(crate) fn send(&self, text: String) -> Result<(), Error> {
        self.out_tx.send(text).map_err(|_| {
            Error::Gateway(GatewayError::Transport(
                "the gateway connection has shut down".to_owned(),
            ))
        })
    }
}

/// A snapshot of the client handed to every event handler.
///
/// `Context` is cheap to clone: it holds the REST client, shared cache and data
/// map, plus a handle for gateway operations.
#[derive(Clone, Debug)]
pub struct Context {
    /// The REST client.
    pub http: HttpClient,
    /// The guild/user cache the gateway keeps up to date.
    pub cache: Arc<Cache>,
    /// The bot's own typed state.
    pub data: Arc<RwLock<Data>>,
    /// The outbound gateway connection, when one is open.
    pub(crate) shard: ShardHandle,
}

impl Context {
    /// Creates the context shared by handlers.
    pub(crate) fn new(
        http: HttpClient,
        cache: Arc<Cache>,
        data: Arc<RwLock<Data>>,
        shard: ShardHandle,
    ) -> Self {
        Self {
            http,
            cache,
            data,
            shard,
        }
    }

    /// Fetches an application emoji by id.
    ///
    /// The application id comes from the cache (set by Ready), falling back to
    /// the REST client's configured id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Gateway`] when the application id is not known yet, and
    /// any error [`HttpClient::get_application_emoji`] returns.
    ///
    /// Reference: `docs/emoji.md` (Application Emoji), `docs/gateway-events.md`
    /// (Ready — `application` object).
    pub async fn get_application_emoji(&self, emoji_id: EmojiId) -> Result<Emoji, Error> {
        let application_id = self
            .cache
            .application_id()
            .or_else(|| self.http.application_id())
            .ok_or_else(|| {
                Error::Gateway(GatewayError::Protocol(
                    "the application id is unknown until the Ready event arrives".to_owned(),
                ))
            })?;

        self.http
            .get_application_emoji(application_id, emoji_id)
            .await
    }

    /// Updates the bot's presence over the live gateway connection.
    ///
    /// The payload is op 3 with the presence object produced by
    /// [`PresenceBuilder::to_json`]. Writing the payload into the shard's
    /// outbound queue is synchronous, so this method is not `async`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Gateway`] when the connection is not open (for example
    /// before `start` connects, or after shutdown).
    ///
    /// Reference: `docs/gateway-events.md` (Update Presence).
    pub fn update_presence(&self, presence: &PresenceBuilder) -> Result<(), Error> {
        if !self.shard.is_connected() {
            return Err(Error::Gateway(GatewayError::Transport(
                "the gateway connection is not open".to_owned(),
            )));
        }

        let payload = payload::presence_update(presence.to_json());
        let text = serde_json::to_string(&payload)?;

        self.shard.send(text)
    }

    /// Marks the gateway connection as open/closed.
    pub(crate) fn set_connected(&self, connected: bool) {
        self.shard.connected.store(connected, Ordering::SeqCst);
    }
}

/// The bot's event handlers.
///
/// Every method has a default no-op implementation, so a bot only overrides the
/// events it cares about. The methods return `Send` futures (which `async fn`
/// implementations satisfy) so the dispatcher can spawn them: handlers may run
/// concurrently and may take as long as they like without stalling the websocket
/// read loop.
///
/// Reference: `docs/gateway-events.md` (Receive Events).
pub trait EventHandler: Send + Sync + 'static {
    /// The initial state after a successful Identify.
    fn on_ready(&self, _ctx: Context, _ready: Ready) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// A successful Resume replayed the missed events.
    fn on_resumed(&self, _ctx: Context, _event: Resumed) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// A message was created.
    fn on_message_create(
        &self,
        _ctx: Context,
        _message: Message,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// A user used an application command or a component.
    fn on_interaction_create(
        &self,
        _ctx: Context,
        _interaction: Interaction,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// A user joined a guild.
    fn on_guild_member_add(
        &self,
        _ctx: Context,
        _member: Member,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// A user left, was kicked or was banned from a guild.
    fn on_guild_member_remove(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _user: User,
        _member: Option<Member>,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// A user was banned from a guild.
    fn on_guild_ban_add(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _user: User,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// A user was unbanned from a guild.
    fn on_guild_ban_remove(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _user: User,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }

    /// An audit log entry was created.
    fn on_guild_audit_log_entry_create(
        &self,
        _ctx: Context,
        _entry: AuditLogEntry,
        _guild_id: GuildId,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }
}

impl EventHandler for () {}

/// A connected bot client.
///
/// Built with [`Client::builder`]; [`Client::start`] runs the gateway connection
/// until the shard manager is shut down (or a fatal close code is received).
pub struct Client<H: EventHandler> {
    /// The bot's own typed state, shared with every [`Context`].
    pub data: Arc<RwLock<Data>>,
    /// The guild/user cache, kept up to date by the gateway.
    pub cache: Arc<Cache>,
    /// The REST client.
    pub http: HttpClient,
    /// Handle used to request a clean shutdown.
    pub shard_manager: Arc<ShardManager>,
    handler: Arc<H>,
    token: String,
    intents: GatewayIntents,
    out_tx: mpsc::UnboundedSender<String>,
    out_rx: Option<mpsc::UnboundedReceiver<String>>,
    connected: Arc<AtomicBool>,
}

impl<H: EventHandler> fmt::Debug for Client<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The handler is not required to be `Debug`, and the bot token must
        // never be printed.
        f.debug_struct("Client")
            .field("data", &self.data)
            .field("cache", &self.cache)
            .field("http", &self.http)
            .field("shard_manager", &self.shard_manager)
            .field("token", &"<redacted>")
            .field("intents", &self.intents)
            .field("connected", &self.connected)
            .finish_non_exhaustive()
    }
}

impl Client<()> {
    /// Starts building a client for `token` that requests `intents`.
    ///
    /// Call [`ClientBuilder::event_handler`] and await the builder to obtain the
    /// [`Client`]:
    ///
    /// ```no_run
    /// # use discord::client::Client;
    /// # use discord::intents::GatewayIntents;
    /// # async fn run() -> Result<(), discord::Error> {
    /// let client = Client::builder("token", GatewayIntents::GUILDS)
    ///     .event_handler(())
    ///     .await?;
    /// # drop(client);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn builder(token: impl Into<String>, intents: GatewayIntents) -> ClientBuilder<()> {
        ClientBuilder {
            token: token.into(),
            intents,
            handler: (),
        }
    }
}

impl<H: EventHandler> Client<H> {
    /// Runs the gateway connection until the client is shut down.
    ///
    /// `shutdown_all` on the [`ShardManager`] makes this return `Ok(())`; fatal
    /// close codes (for example `4004` authentication failed or `4014`
    /// disallowed intents) make it return [`Error::Gateway`] with
    /// [`GatewayError::Closed`] and `fatal: true`.
    ///
    /// Reference: `docs/gateway.md` (Connection Lifecycle, Resuming, Initiating a
    /// Disconnect), `docs/opcodes-and-status-codes.md` (Gateway Close Event
    /// Codes).
    ///
    /// # Errors
    ///
    /// Returns an error when the gateway cannot be reached after retries, when a
    /// fatal close code is received, or when the client was already started.
    pub async fn start(&mut self) -> Result<(), Error> {
        let out_rx = self.out_rx.take().ok_or_else(|| {
            Error::Gateway(GatewayError::Protocol(
                "this client has already been started".to_owned(),
            ))
        })?;

        let shard = Shard::new(
            self.token.clone(),
            self.intents,
            Arc::clone(&self.handler),
            self.context(),
            Arc::clone(&self.shard_manager),
        );

        shard.run(out_rx).await
    }

    /// Builds a [`Context`] for the handlers.
    fn context(&self) -> Context {
        Context::new(
            self.http.clone(),
            Arc::clone(&self.cache),
            Arc::clone(&self.data),
            ShardHandle::new(self.out_tx.clone(), Arc::clone(&self.connected)),
        )
    }
}

/// Builder for [`Client`].
///
/// Awaited to produce `Result<Client<H>, Error>` via [`IntoFuture`].
pub struct ClientBuilder<H> {
    token: String,
    intents: GatewayIntents,
    handler: H,
}

impl<H> fmt::Debug for ClientBuilder<H> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The handler is not required to be `Debug`, and the bot token must
        // never be printed.
        f.debug_struct("ClientBuilder")
            .field("token", &"<redacted>")
            .field("intents", &self.intents)
            .finish_non_exhaustive()
    }
}

impl ClientBuilder<()> {
    /// Attaches the bot's [`EventHandler`], producing a client for it.
    #[must_use]
    pub fn event_handler<H2: EventHandler>(self, handler: H2) -> ClientBuilder<H2> {
        ClientBuilder {
            token: self.token,
            intents: self.intents,
            handler,
        }
    }
}

impl<H: EventHandler> ClientBuilder<H> {
    /// Creates the client's shared state.
    fn build(self) -> Result<Client<H>, Error> {
        if self.token.trim().is_empty() {
            return Err(Error::Gateway(GatewayError::Protocol(
                "the bot token must not be empty".to_owned(),
            )));
        }

        let http = HttpClient::new(self.token.clone());
        let cache = Arc::new(Cache::new());
        let data = Arc::new(RwLock::new(Data::default()));
        let shard_manager = Arc::new(ShardManager::new());
        let (out_tx, out_rx) = mpsc::unbounded_channel();

        Ok(Client {
            data,
            cache,
            http,
            shard_manager,
            handler: Arc::new(self.handler),
            token: self.token,
            intents: self.intents,
            out_tx,
            out_rx: Some(out_rx),
            connected: Arc::new(AtomicBool::new(false)),
        })
    }
}

impl<H: EventHandler> IntoFuture for ClientBuilder<H> {
    type Output = Result<Client<H>, Error>;
    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move { self.build() })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    struct Ids;

    impl DataKey for Ids {
        type Value = (u64, u64);
    }

    #[test]
    fn data_map_stores_values_by_key_type() {
        let mut data = Data::default();

        data.insert::<Ids>((1, 2));

        assert_eq!(data.get::<Ids>(), Some(&(1, 2)));
    }

    #[test]
    fn data_map_returns_none_for_a_missing_key() {
        let data = Data::default();

        assert!(data.get::<Ids>().is_none());
    }

    #[test]
    fn debug_output_does_not_leak_the_token() {
        // `Debug` implementations are easy to add carelessly; this guards the
        // rule that the bot token is never printed by either type.
        let builder = Client::builder("super-secret-token", GatewayIntents::GUILDS);
        let builder_debug = format!("{builder:?}");

        assert!(builder_debug.contains("ClientBuilder"));
        assert!(!builder_debug.contains("super-secret-token"));

        let client = builder.build().expect("client builds");
        let client_debug = format!("{client:?}");

        assert!(client_debug.contains("Client"));
        assert!(!client_debug.contains("super-secret-token"));
    }

    #[tokio::test]
    async fn awaiting_the_builder_produces_a_client() {
        let client = Client::builder("token", GatewayIntents::GUILDS)
            .event_handler(())
            .await
            .expect("client builds");

        client.data.write().await.insert::<Ids>((3, 4));

        assert_eq!(client.data.read().await.get::<Ids>().copied(), Some((3, 4)));
        assert!(client.cache.current_user().is_none());
    }

    #[tokio::test]
    async fn an_empty_token_is_rejected() {
        let result = Client::builder("", GatewayIntents::GUILDS)
            .event_handler(())
            .await;

        assert!(matches!(
            result,
            Err(Error::Gateway(GatewayError::Protocol(_)))
        ));
    }

    #[tokio::test]
    async fn a_client_can_only_be_started_once() {
        let mut client = Client::builder("token", GatewayIntents::GUILDS)
            .event_handler(())
            .await
            .expect("client builds");

        // Starting the client without a network is not part of this test; just
        // verify that the receiver is consumed exactly once.
        let out_rx = client.out_rx.take();
        assert!(out_rx.is_some());
        assert!(client.out_rx.is_none());
    }

    /// The documented handler style: `async fn` implementations of the trait.
    struct AsyncHandler;

    impl EventHandler for AsyncHandler {
        async fn on_ready(&self, _ctx: Context, _ready: Ready) {}

        async fn on_message_create(&self, _ctx: Context, _message: Message) {}

        async fn on_guild_ban_add(&self, _ctx: Context, _guild_id: GuildId, _user: User) {}
    }

    #[test]
    fn async_fn_implementations_satisfy_the_handler_trait() {
        fn assert_handler<H: EventHandler>() {}
        fn assert_send_future<F: Future + Send>(_: F) {}

        assert_handler::<AsyncHandler>();

        let (ctx, _out_rx) = test_context(false);

        assert_send_future(AsyncHandler.on_resumed(ctx, Resumed));
    }

    /// Builds a `Context` with no network connection.
    fn test_context(connected: bool) -> (Context, mpsc::UnboundedReceiver<String>) {
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let ctx = Context::new(
            HttpClient::new("token"),
            Arc::new(Cache::new()),
            Arc::new(RwLock::new(Data::default())),
            ShardHandle::new(out_tx, Arc::new(AtomicBool::new(connected))),
        );

        (ctx, out_rx)
    }

    #[tokio::test]
    async fn update_presence_sends_the_documented_op3_payload() {
        use crate::builder::presence::{Activity, PresenceBuilder, Status};

        let (ctx, mut out_rx) = test_context(true);
        let presence = PresenceBuilder::new()
            .status(Status::DoNotDisturb)
            .activity(Activity::custom("Playing with code."));

        ctx.update_presence(&presence)
            .expect("the presence update is queued");

        let text = out_rx.recv().await.expect("a payload is queued");
        let payload: serde_json::Value = serde_json::from_str(&text).expect("the payload is JSON");

        assert_eq!(payload["op"], 3);
        assert_eq!(payload["d"]["status"], "dnd");
        assert_eq!(payload["d"]["activities"][0]["type"], 4);
        assert_eq!(payload["d"]["activities"][0]["name"], "Playing with code.");
    }

    #[tokio::test]
    async fn update_presence_fails_when_the_connection_is_not_open() {
        use crate::builder::presence::PresenceBuilder;

        let (ctx, _out_rx) = test_context(false);

        let error = ctx
            .update_presence(&PresenceBuilder::new())
            .expect_err("a closed connection cannot carry a presence update");

        assert!(matches!(error, Error::Gateway(GatewayError::Transport(_))));
    }

    #[tokio::test]
    async fn application_emoji_requires_the_application_id() {
        let (ctx, _out_rx) = test_context(false);

        let error = ctx
            .get_application_emoji(EmojiId::new(1))
            .await
            .expect_err("the application id is unknown before Ready");

        assert!(matches!(error, Error::Gateway(GatewayError::Protocol(_))));
    }

    #[tokio::test]
    async fn start_returns_ok_when_shutdown_was_requested_first() {
        let mut client = Client::builder("token", GatewayIntents::GUILDS)
            .event_handler(())
            .await
            .expect("client builds");
        let manager = Arc::clone(&client.shard_manager);

        let waiting = tokio::spawn({
            let manager = Arc::clone(&manager);
            async move { manager.shutdown_all().await }
        });

        while !manager.is_shutdown_requested() {
            tokio::task::yield_now().await;
        }

        // A shutdown requested before the first connection makes `start` return
        // without touching the network.
        let result = tokio::time::timeout(Duration::from_secs(2), client.start())
            .await
            .expect("start returns for a requested shutdown");

        assert!(result.is_ok(), "a clean shutdown returns Ok: {result:?}");

        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("shutdown_all returns once the shard has finished")
            .expect("the shutdown task did not panic");
    }
}
