//! A single-guild Discord moderation bot.
//!
//! The bot token is read from the `DISCORD_TOKEN` environment variable at
//! start-up and is never compiled into the binary, so release artifacts are
//! safe to publish.
//!
//! The Discord protocol itself is implemented by this repository's own
//! `discord` crate (`crates/discord`), written against the official Discord API
//! reference; a local copy of that reference is not vendored here.

mod commands;
mod config;
mod error;
mod events;
mod moderation;
mod options;

use discord::Client;
use discord::GatewayIntents;

use tracing::error;
use tracing::info;
use tracing_subscriber::filter;

/// Gateway intents the bot actually needs.
///
/// * [`GUILDS`] populates the guild cache. It is **not** optional here: the
///   cache is filled from `GUILD_CREATE`, so without it `ctx.cache.guild()`
///   stays empty forever and every cache-backed check (notably the permission
///   gate in `src/options.rs`) fails closed, permanently denying admins too.
/// * [`GUILD_MESSAGES`] + [`MESSAGE_CONTENT`] for `events::message`, which reads
///   `Message::content`.
/// * [`GUILD_MEMBERS`] for the member add/remove events.
/// * [`GUILD_MODERATION`] for the ban events and the audit log entry event.
///
/// [`GUILD_PRESENCES`] and the rest of the documented intent bits are
/// deliberately *not* requested: each privileged intent must be enabled in the
/// Developer Portal, and requesting one that is not enabled makes Discord close
/// the gateway with code 4014 (disallowed intents), which is fatal.
///
/// Reference: `docs/gateway.md` (Gateway Intents) and `docs/gateway-events.md`
/// (Identify).
///
/// [`GUILDS`]: GatewayIntents::GUILDS
/// [`GUILD_MESSAGES`]: GatewayIntents::GUILD_MESSAGES
/// [`MESSAGE_CONTENT`]: GatewayIntents::MESSAGE_CONTENT
/// [`GUILD_MEMBERS`]: GatewayIntents::GUILD_MEMBERS
/// [`GUILD_MODERATION`]: GatewayIntents::GUILD_MODERATION
/// [`GUILD_PRESENCES`]: GatewayIntents::GUILD_PRESENCES
const INTENTS: GatewayIntents = GatewayIntents::GUILDS
    .union(GatewayIntents::GUILD_MESSAGES)
    .union(GatewayIntents::MESSAGE_CONTENT)
    .union(GatewayIntents::GUILD_MEMBERS)
    .union(GatewayIntents::GUILD_MODERATION);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_target(false)
        .with_max_level(filter::LevelFilter::INFO)
        .with_ansi(false)
        .init();

    // The token is read from the environment so it is never committed and never
    // baked into a release binary.
    let config = match config::Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            error!("{err}");
            std::process::exit(1);
        }
    };

    let ids = config.ids;

    let mut client = match Client::builder(config.token.as_str(), INTENTS)
        .event_handler(events::Handler::new(ids))
        .await
    {
        Ok(client) => client,
        Err(err) => {
            error!("failed to create client: {err}");
            std::process::exit(1);
        }
    };

    // Publish the ids and the dashboard content so command handlers can read
    // them without threading parameters through every `run()` signature. The
    // client's data map is a typed map: `config::Ids` and `config::Dashboard`
    // implement `discord::DataKey`.
    client.data.write().await.insert::<config::Ids>(ids);
    client
        .data
        .write()
        .await
        .insert::<config::Dashboard>(config.dashboard);

    // Held before `start()` consumes the client: this is the handle used to ask
    // the shard to shut down once a termination signal arrives.
    let shard_manager = client.shard_manager.clone();

    // `start()` only returns when the connection ends, so it cannot simply be
    // awaited if we also want to react to Ctrl+C. The task reports the error
    // rather than exiting the process itself, otherwise a failed start would
    // leave `main` waiting on a signal that is never coming.
    let mut client_task = tokio::spawn(async move { client.start().await });

    tokio::select! {
        // A termination signal arrived: ask the shard to close, then wait for
        // the client task to finish so the gateway connection is closed cleanly
        // rather than dropped mid-frame.
        () = shutdown_signal() => {
            info!("shutdown signal received, closing gateway connections");
            shard_manager.shutdown_all().await;

            // `&mut JoinHandle` is itself a future, so the handle stays usable.
            // The timeout is what makes this branch terminate: if the task already
            // finished there is nothing to wait for, and if it is stuck the wait
            // is abandoned rather than hanging the process.
            match tokio::time::timeout(std::time::Duration::from_secs(10), &mut client_task).await {
                Ok(Ok(Ok(()))) => info!("shutdown complete"),
                Ok(Ok(Err(err))) => error!("client stopped with an error: {err}"),
                Ok(Err(err)) => error!("client task failed: {err}"),
                Err(_) => {
                    error!("client did not stop within 10s, exiting anyway");
                    std::process::exit(1);
                }
            }
        }

        // The client ended on its own: nothing left to serve, so exit non-zero
        // for a failure and cleanly for a normal stop.
        result = &mut client_task => match result {
            Ok(Ok(())) => info!("client stopped"),
            Ok(Err(err)) => {
                error!("client stopped: {err}");
                std::process::exit(1);
            }
            Err(err) => {
                error!("client task failed: {err}");
                std::process::exit(1);
            }
        },
    }
}

/// Resolves when the process is asked to terminate.
///
/// Listens for both Ctrl+C and, on Unix, `SIGTERM`, so `docker stop` and systemd
/// shutdowns also close the gateway cleanly instead of dropping the connection.
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            error!("failed to listen for Ctrl+C: {err}");
            // Never resolve: without a listener there is nothing to act on.
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(err) => {
                error!("failed to listen for SIGTERM: {err}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
