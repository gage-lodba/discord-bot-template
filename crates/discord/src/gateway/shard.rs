//! The gateway connection state machine.
//!
//! One [`Shard`] owns one websocket connection at a time and keeps the values a
//! Resume needs (session id, `resume_gateway_url` and the last sequence number).
//! Its lifecycle follows `docs/gateway.md` (Connection Lifecycle):
//!
//! 1. connect and wait for Hello (op 10),
//! 2. start the jittered heartbeat loop and send Identify (op 2) or Resume (op 6),
//! 3. read events until the server reconnects (op 7), invalidates the session
//!    (op 9), closes the connection, or the client is asked to shut down,
//! 4. reconnect with exponential backoff (resuming when a session exists), or
//!    return the fatal error so `Client::start` stops.
//!
//! Reference: `docs/gateway.md` (Connection Lifecycle, Sending Heartbeats,
//! Identifying, Resuming, Disconnecting), `docs/gateway-events.md` (Heartbeat,
//! Identify, Resume, Reconnect, Invalid Session),
//! `docs/opcodes-and-status-codes.md` (Gateway Opcodes, Gateway Close Event
//! Codes).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use crate::client::{Context, EventHandler};
use crate::error::{Error, GatewayError};
use crate::gateway::close::{self, CloseAction};
use crate::gateway::dispatch;
use crate::gateway::payload::{self, GatewayPayload, Hello, Opcode};
use crate::gateway::shard_manager::ShardManager;
use crate::intents::GatewayIntents;
use crate::model::ready::Ready;

/// The URL a fresh gateway connection is opened against.
///
/// `wss://gateway.discord.gg/?v=10&encoding=json` is the example the reference
/// gives; the client asks for JSON and no transport compression.
///
/// Reference: `docs/gateway.md` (Connecting — Gateway URL Query String Params).
pub(crate) const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

/// Query string appended to a `resume_gateway_url`.
const GATEWAY_QUERY: &str = "?v=10&encoding=json";

/// Timeout for the TCP/TLS/WebSocket handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for the Hello payload Discord sends immediately after connecting.
const HELLO_TIMEOUT: Duration = Duration::from_secs(30);

/// The websocket type used for a gateway connection.
type GatewaySocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// One inbound websocket frame, or the error that ended the stream.
type InboundFrame = Option<Result<Message, tokio_tungstenite::tungstenite::Error>>;

/// The session values needed to Resume after a disconnect.
///
/// The struct is named after what the values are for (resuming) so its
/// documented `session_id` field keeps Discord's terminology.
///
/// Reference: `docs/gateway.md` (Preparing to Resume).
struct ResumeState {
    session_id: String,
    resume_url: String,
    seq: Option<i64>,
}

/// What to do after a connection attempt ends.
enum Outcome {
    /// The client was asked to shut down; `Client::start` returns `Ok(())`.
    Shutdown,
    /// Open a new connection (after an optional delay).
    Reconnect {
        /// Keep the session and send Resume, or discard it and Identify again.
        resume: bool,
        /// A delay prescribed by the protocol (Invalid Session); when `None` the
        /// reconnect backoff is used instead.
        delay: Option<Duration>,
        /// Whether the connection reached Ready/Resumed (resets the backoff).
        reset_backoff: bool,
    },
}

/// The result of waiting for the gateway Hello, which a shutdown can interrupt.
enum HelloOutcome {
    /// Hello arrived; the connection can be used.
    Hello(Hello),
    /// The gateway asked for a reconnect before it sent Hello.
    ///
    /// Reference: `docs/gateway-events.md` (Reconnect — "This can occur at any
    /// point in the gateway connection lifecycle, even before/in place of
    /// receiving a Hello event.").
    Reconnect,
    /// A shutdown was requested while connecting.
    Shutdown,
}

/// One step of a connection attempt: carry on with a value, or end the attempt
/// with the [`Outcome`] that tells the shard what to do next.
enum Step<T> {
    /// Keep going with the value the step produced.
    Continue(T),
    /// Stop this connection attempt and act on the outcome.
    Ended(Outcome),
}

/// The per-connection state the read loop maintains.
#[derive(Debug, Default)]
struct ConnectionState {
    /// True between sending a heartbeat and receiving its acknowledgement.
    awaiting_ack: bool,
    /// True once Ready/Resumed established the session on this connection.
    established: bool,
}

/// One gateway connection.
pub(crate) struct Shard<H: EventHandler> {
    token: String,
    intents: GatewayIntents,
    handler: Arc<H>,
    ctx: Context,
    manager: Arc<ShardManager>,
    session: Option<ResumeState>,
    /// URL used for a fresh connection. Always [`GATEWAY_URL`] outside tests,
    /// where it is pointed at an in-process mock gateway.
    gateway_url: String,
}

impl<H: EventHandler> Shard<H> {
    /// Creates a shard that has not connected yet.
    pub(crate) fn new(
        token: String,
        intents: GatewayIntents,
        handler: Arc<H>,
        ctx: Context,
        manager: Arc<ShardManager>,
    ) -> Self {
        Self {
            token,
            intents,
            handler,
            ctx,
            manager,
            session: None,
            gateway_url: GATEWAY_URL.to_owned(),
        }
    }

    /// Runs the shard until it is shut down or hits a fatal close code.
    pub(crate) async fn run(
        mut self,
        mut out_rx: mpsc::UnboundedReceiver<String>,
    ) -> Result<(), Error> {
        let result = self.run_inner(&mut out_rx).await;

        self.ctx.set_connected(false);
        self.manager.mark_finished();

        result
    }

    /// Reconnects until the shard is shut down or fails fatally.
    ///
    /// The reference asks the client to reconnect and Resume *immediately* after
    /// a reconnectable disconnect of an established session (opcode 7 Reconnect,
    /// a reconnectable close code, or a dropped connection), while a new session
    /// (re-identify) and repeated consecutive connection failures use the
    /// exponential backoff (`docs/gateway.md`, Handling a Disconnect / Resuming;
    /// `docs/opcodes-and-status-codes.md`, Reconnect).
    async fn run_inner(
        &mut self,
        out_rx: &mut mpsc::UnboundedReceiver<String>,
    ) -> Result<(), Error> {
        let mut schedule = ReconnectSchedule::new();

        loop {
            if self.manager.is_shutdown_requested() {
                return Ok(());
            }

            match self.connect_once(out_rx).await {
                Ok(Outcome::Shutdown) => return Ok(()),
                Ok(Outcome::Reconnect {
                    resume,
                    delay,
                    reset_backoff,
                }) => {
                    if !resume {
                        self.session = None;
                    }
                    if reset_backoff {
                        schedule.session_established();
                    }

                    let wait = schedule.next_delay(resume, delay);
                    tracing::debug!(
                        resume,
                        wait_ms = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX),
                        "reconnecting to the gateway"
                    );

                    if wait > Duration::ZERO && self.wait_or_shutdown(wait).await {
                        return Ok(());
                    }
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Sleeps for `duration`, returning true when a shutdown is requested first.
    async fn wait_or_shutdown(&self, duration: Duration) -> bool {
        let mut shutdown = self.manager.subscribe_shutdown();
        if *shutdown.borrow() {
            return true;
        }

        tokio::select! {
            () = tokio::time::sleep(duration) => false,
            result = shutdown.changed() => result.is_ok() && *shutdown.borrow(),
        }
    }

    /// Opens one connection and reads events from it until it ends.
    async fn connect_once(
        &mut self,
        out_rx: &mut mpsc::UnboundedReceiver<String>,
    ) -> Result<Outcome, Error> {
        let outcome = self.connect_once_inner(out_rx).await;

        self.ctx.set_connected(false);
        outcome
    }

    /// The body of [`Shard::connect_once`]: open the connection, perform the
    /// handshake and read events until it ends.
    ///
    /// Each stage has its own helper so the handshake's shutdown/error handling
    /// stays separate from the heartbeat-driven read loop.
    async fn connect_once_inner(
        &mut self,
        out_rx: &mut mpsc::UnboundedReceiver<String>,
    ) -> Result<Outcome, Error> {
        let mut shutdown = self.manager.subscribe_shutdown();
        if *shutdown.borrow() {
            return Ok(Outcome::Shutdown);
        }

        let mut socket = match self.open_socket(&mut shutdown).await? {
            Step::Continue(socket) => socket,
            Step::Ended(outcome) => return Ok(outcome),
        };

        let hello = match self.exchange_handshake(&mut socket, &mut shutdown).await? {
            Step::Continue(hello) => hello,
            Step::Ended(outcome) => return Ok(outcome),
        };

        let mut heartbeat = heartbeat_timer(hello.interval());
        let mut state = ConnectionState::default();

        // The connection only counts as open once Identify/Resume is on the
        // wire; presence updates before that would be lost.
        if let Err(error) = send_json(&mut socket, &self.first_payload()).await {
            return self.error_or_reconnect(error, state.established);
        }
        self.ctx.set_connected(true);

        self.read_events(
            &mut socket,
            &mut shutdown,
            out_rx,
            &mut heartbeat,
            &mut state,
        )
        .await
    }

    /// Opens one websocket connection, mapping a failed or timed-out handshake
    /// to the reconnecting [`Outcome`].
    async fn open_socket(
        &self,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<Step<GatewaySocket>, Error> {
        let url = connection_url(self.session.as_ref(), &self.gateway_url);
        tracing::debug!(%url, "opening a gateway connection");

        tokio::select! {
            result = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(url)) => {
                match result {
                    Ok(Ok((socket, _response))) => Ok(Step::Continue(socket)),
                    Ok(Err(error)) => self
                        .error_or_reconnect(
                            Error::Gateway(GatewayError::Transport(format!(
                                "could not connect to the gateway: {error}"
                            ))),
                            false,
                        )
                        .map(Step::Ended),
                    Err(_) => self
                        .error_or_reconnect(
                            Error::Gateway(GatewayError::Transport(
                                "timed out connecting to the gateway".to_owned(),
                            )),
                            false,
                        )
                        .map(Step::Ended),
                }
            }
            _ = shutdown.changed() => {
                tracing::debug!("shutting down before the gateway connection opened");
                Ok(Step::Ended(Outcome::Shutdown))
            }
        }
    }

    /// Waits for Hello and reports whether the connection may continue.
    ///
    /// A Reconnect (op 7) can arrive before/in place of Hello
    /// (`docs/gateway-events.md`, Reconnect); the attempt then ends with a
    /// reconnect instead of waiting for the Hello timeout.
    async fn exchange_handshake(
        &self,
        socket: &mut GatewaySocket,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<Step<Hello>, Error> {
        match self.wait_for_hello(socket, shutdown).await {
            Ok(HelloOutcome::Hello(hello)) => Ok(Step::Continue(hello)),
            Ok(HelloOutcome::Reconnect) => {
                // "You should attempt to reconnect and resume immediately"
                // (`docs/opcodes-and-status-codes.md`, Gateway Opcodes).
                tracing::debug!("the gateway asked us to reconnect before sending Hello");
                Ok(Step::Ended(Outcome::Reconnect {
                    resume: self.session.is_some(),
                    delay: None,
                    reset_backoff: false,
                }))
            }
            Ok(HelloOutcome::Shutdown) => Ok(Step::Ended(Outcome::Shutdown)),
            Err(error) => self.error_or_reconnect(error, false).map(Step::Ended),
        }
    }

    /// The heartbeat-driven read loop of one live connection.
    ///
    /// Returns the [`Outcome`] that ends the attempt: a protocol event, a close
    /// code, a transport failure or a shutdown request.
    async fn read_events(
        &mut self,
        socket: &mut GatewaySocket,
        shutdown: &mut watch::Receiver<bool>,
        out_rx: &mut mpsc::UnboundedReceiver<String>,
        heartbeat: &mut tokio::time::Interval,
        state: &mut ConnectionState,
    ) -> Result<Outcome, Error> {
        loop {
            if self.manager.is_shutdown_requested() {
                close_cleanly(socket).await;
                return Ok(Outcome::Shutdown);
            }

            // `biased` checks the shutdown signal first and the heartbeat timer
            // before the socket, preserving the pre-split ordering.
            tokio::select! {
                biased;
                result = shutdown.changed() => {
                    if result.is_ok() && *shutdown.borrow() {
                        close_cleanly(socket).await;
                        return Ok(Outcome::Shutdown);
                    }
                }
                outbound = out_rx.recv() => {
                    if let Some(outcome) = self.relay_outbound(socket, outbound, state).await? {
                        return Ok(outcome);
                    }
                }
                _ = heartbeat.tick() => {
                    if let Some(outcome) = self.handle_heartbeat_tick(socket, state).await? {
                        return Ok(outcome);
                    }
                }
                message = socket.next() => {
                    if let Some(outcome) = self.handle_inbound(socket, message, state).await? {
                        return Ok(outcome);
                    }
                }
            }
        }
    }

    /// Writes one queued client payload to the gateway.
    ///
    /// Returns `Ok(None)` to keep the connection open, or the [`Outcome`] that
    /// ends it when the sending half is gone or the write fails.
    async fn relay_outbound(
        &self,
        socket: &mut GatewaySocket,
        outbound: Option<String>,
        state: &ConnectionState,
    ) -> Result<Option<Outcome>, Error> {
        let Some(text) = outbound else {
            // The client that owns the sending half is gone; there is nothing
            // left to do but shut down.
            close_cleanly(socket).await;
            return Ok(Some(Outcome::Shutdown));
        };

        if let Err(error) = socket.send(Message::text(text)).await {
            return self
                .error_or_reconnect(
                    Error::Gateway(GatewayError::Transport(format!(
                        "could not send a gateway payload: {error}"
                    ))),
                    state.established,
                )
                .map(Some);
        }

        Ok(None)
    }

    /// Handles one heartbeat timer tick: sends a heartbeat, or reconnects when
    /// the previous one was never acknowledged.
    async fn handle_heartbeat_tick(
        &self,
        socket: &mut GatewaySocket,
        state: &mut ConnectionState,
    ) -> Result<Option<Outcome>, Error> {
        match heartbeat_decision(state.awaiting_ack) {
            HeartbeatDecision::Zombie => {
                tracing::debug!("the last heartbeat was not acknowledged; reconnecting");
                close_with(
                    socket,
                    close::ZOMBIE_CLOSE_CODE,
                    "heartbeat not acknowledged",
                )
                .await;

                Ok(Some(Outcome::Reconnect {
                    resume: self.session.is_some(),
                    delay: None,
                    reset_backoff: state.established,
                }))
            }
            HeartbeatDecision::Send => {
                if let Err(error) = send_json(socket, &self.heartbeat_payload()).await {
                    return self.error_or_reconnect(error, state.established).map(Some);
                }
                state.awaiting_ack = true;

                Ok(None)
            }
        }
    }

    /// Decides what to do with one inbound websocket frame.
    ///
    /// Returns `Ok(None)` when the connection can continue reading.
    async fn handle_inbound(
        &mut self,
        socket: &mut GatewaySocket,
        message: InboundFrame,
        state: &mut ConnectionState,
    ) -> Result<Option<Outcome>, Error> {
        match message {
            Some(Ok(Message::Text(text))) => self.handle_event(socket, &text, state).await,
            Some(Ok(Message::Close(frame))) => {
                let (code, reason) = close_parts(frame);
                self.handle_close(code, &reason, state.established)
                    .map(Some)
            }
            Some(Ok(Message::Ping(data))) => {
                if let Err(error) = socket.send(Message::Pong(data)).await {
                    return self
                        .error_or_reconnect(
                            Error::Gateway(GatewayError::Transport(format!(
                                "could not answer a gateway ping: {error}"
                            ))),
                            state.established,
                        )
                        .map(Some);
                }

                Ok(None)
            }
            Some(Ok(Message::Pong(_) | Message::Binary(_) | Message::Frame(_))) => Ok(None),
            Some(Err(error)) => self
                .error_or_reconnect(
                    Error::Gateway(GatewayError::Transport(format!(
                        "gateway websocket error: {error}"
                    ))),
                    state.established,
                )
                .map(Some),
            None => {
                // No close frame: the session may still be resumable
                // (`docs/gateway.md`, Disconnecting / Resuming).
                tracing::debug!("the gateway connection ended without a close frame");

                Ok(Some(Outcome::Reconnect {
                    resume: self.session.is_some(),
                    delay: None,
                    reset_backoff: state.established,
                }))
            }
        }
    }

    /// Decodes one text payload and routes its opcode to the session, heartbeat
    /// or dispatch logic.
    ///
    /// Returns `Ok(None)` when the connection can continue reading.
    async fn handle_event(
        &mut self,
        socket: &mut GatewaySocket,
        text: &str,
        state: &mut ConnectionState,
    ) -> Result<Option<Outcome>, Error> {
        let event = match payload::decode(text) {
            Ok(event) => event,
            Err(error) => {
                tracing::warn!(%error, "ignoring an undecodable gateway payload");
                return Ok(None);
            }
        };

        match Opcode::from_i64(event.op) {
            Some(Opcode::HeartbeatAck) => state.awaiting_ack = false,
            Some(Opcode::Heartbeat) => {
                // The server asked for a heartbeat: answer right away.
                if let Err(error) = send_json(socket, &self.heartbeat_payload()).await {
                    return self.error_or_reconnect(error, state.established).map(Some);
                }
                state.awaiting_ack = true;
            }
            Some(Opcode::Reconnect) => {
                // Do not send a close frame: the session must stay resumable
                // (`docs/gateway.md`, Disconnecting).
                tracing::debug!("the gateway asked us to reconnect");

                return Ok(Some(Outcome::Reconnect {
                    resume: self.session.is_some(),
                    delay: None,
                    reset_backoff: state.established,
                }));
            }
            Some(Opcode::InvalidSession) => {
                let resumable = event.d.as_bool().unwrap_or(false);
                if resumable {
                    tracing::debug!("the session was invalidated; resuming");

                    return Ok(Some(Outcome::Reconnect {
                        resume: true,
                        delay: None,
                        reset_backoff: state.established,
                    }));
                }

                tracing::debug!("the session could not be resumed; identifying again");
                self.session = None;

                return Ok(Some(Outcome::Reconnect {
                    resume: false,
                    delay: Some(reidentify_delay(random_seed())),
                    reset_backoff: false,
                }));
            }
            Some(Opcode::Dispatch) => {
                self.handle_dispatch(&event, &mut state.established);
            }
            Some(Opcode::Hello) => {
                tracing::debug!("ignoring a duplicate Hello payload");
            }
            Some(other) => {
                tracing::debug!(?other, "ignoring an unexpected gateway opcode");
            }
            None => {
                tracing::debug!(op = event.op, "ignoring an unknown gateway opcode");
            }
        }

        Ok(None)
    }

    /// Builds a Heartbeat (op 1) payload carrying the last sequence number.
    fn heartbeat_payload(&self) -> Value {
        payload::heartbeat(self.session.as_ref().and_then(|session| session.seq))
    }

    /// Builds the first payload of a connection: Resume when a session is known,
    /// Identify otherwise.
    fn first_payload(&mut self) -> Value {
        let resume = self
            .session
            .as_ref()
            .and_then(|session| session.seq.map(|seq| (session.session_id.clone(), seq)));

        if let Some((session_id, seq)) = resume {
            payload::resume(&self.token, &session_id, seq)
        } else {
            self.session = None;
            payload::identify(&self.token, self.intents, None, None)
        }
    }

    /// Reads frames until Hello arrives, or until a shutdown is requested.
    async fn wait_for_hello(
        &self,
        socket: &mut GatewaySocket,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<HelloOutcome, Error> {
        let deadline = tokio::time::sleep(HELLO_TIMEOUT);
        tokio::pin!(deadline);

        loop {
            if *shutdown.borrow() {
                return Ok(HelloOutcome::Shutdown);
            }

            tokio::select! {
                () = &mut deadline => {
                    return Err(Error::Gateway(GatewayError::Transport(
                        "timed out waiting for the gateway Hello".to_owned(),
                    )));
                }
                result = shutdown.changed() => {
                    if result.is_ok() && *shutdown.borrow() {
                        tracing::debug!("shutting down while waiting for the gateway Hello");
                        return Ok(HelloOutcome::Shutdown);
                    }
                }
                message = socket.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            let event = payload::decode(&text)?;
                            let opcode = Opcode::from_i64(event.op);

                            if opcode == Some(Opcode::Hello) {
                                return Hello::parse(&event).map(HelloOutcome::Hello);
                            }

                            if opcode == Some(Opcode::Reconnect) {
                                // `docs/gateway-events.md` (Reconnect): the event
                                // can be sent "even before/in place of receiving a
                                // Hello event".
                                return Ok(HelloOutcome::Reconnect);
                            }

                            tracing::debug!(
                                op = event.op,
                                "ignoring a payload received before the gateway Hello"
                            );
                        }
                        Some(Ok(Message::Close(frame))) => {
                            let (code, reason) = close_parts(frame);
                            return Err(close_error(code, reason));
                        }
                        Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_))) => {}
                        Some(Err(error)) => {
                            return Err(Error::Gateway(GatewayError::Transport(format!(
                                "gateway websocket error: {error}"
                            ))));
                        }
                        None => {
                            return Err(Error::Gateway(GatewayError::Transport(
                                "the gateway closed before sending Hello".to_owned(),
                            )));
                        }
                    }
                }
            }
        }
    }

    /// Handles one dispatch event: track the sequence number, keep the session
    /// values a Resume needs, then update the cache and call the handler.
    fn handle_dispatch(&mut self, event: &GatewayPayload, established: &mut bool) {
        if let (Some(seq), Some(session)) = (event.s, self.session.as_mut()) {
            session.seq = Some(seq);
        }

        let Some(event_name) = event.t.as_deref() else {
            tracing::debug!("ignoring a dispatch payload without an event name");
            return;
        };

        if event_name == "READY" {
            match serde_json::from_value::<Ready>(event.d.clone()) {
                Ok(ready) => {
                    self.session = Some(ResumeState {
                        session_id: ready.session_id,
                        resume_url: ready.resume_gateway_url,
                        seq: event.s,
                    });
                    *established = true;
                }
                Err(error) => {
                    tracing::warn!(%error, "could not read the Ready event");
                }
            }
        } else if event_name == "RESUMED" {
            *established = true;
        }

        if let Err(error) = dispatch::handle(&self.ctx, &self.handler, event_name, event.d.clone())
        {
            tracing::warn!(%error, event = event_name, "could not process a gateway event");
        }
    }

    /// Turns a close code into the next action.
    ///
    /// Clean `1000`/`1001` closes are only a real shutdown when `shutdown_all`
    /// was called: the reference reserves those codes for client-initiated
    /// disconnects, so a server-sent `1000`/`1001` is treated like any other
    /// reconnectable close (`docs/gateway.md`, Initiating a Disconnect).
    fn handle_close(&self, code: u16, reason: &str, established: bool) -> Result<Outcome, Error> {
        close_outcome(
            code,
            reason,
            self.session.is_some(),
            established,
            self.manager.is_shutdown_requested(),
        )
    }

    /// Logs a connection error and decides whether to retry it.
    fn error_or_reconnect(&self, error: Error, established: bool) -> Result<Outcome, Error> {
        reconnect_after_error(error, self.session.is_some(), established)
    }
}

/// Decides whether a connection error is fatal or should be retried.
fn reconnect_after_error(
    error: Error,
    session_present: bool,
    established: bool,
) -> Result<Outcome, Error> {
    if matches!(
        &error,
        Error::Gateway(GatewayError::Closed { fatal: true, .. })
    ) {
        return Err(error);
    }

    tracing::debug!(%error, "the gateway connection failed; reconnecting");

    Ok(Outcome::Reconnect {
        resume: session_present,
        delay: None,
        reset_backoff: established,
    })
}

/// Builds the URL for the next connection attempt.
///
/// Fresh connections use the cached `/gateway` URL; reconnects use the
/// `resume_gateway_url` from Ready with the same version and encoding query
/// parameters (`docs/gateway.md`, Preparing to Resume).
fn connection_url(session: Option<&ResumeState>, fresh_url: &str) -> String {
    let Some(session) = session else {
        return fresh_url.to_owned();
    };

    let base = session.resume_url.trim_end_matches('/');
    if base.contains('?') {
        base.to_owned()
    } else {
        format!("{base}/{GATEWAY_QUERY}")
    }
}

/// Converts a close frame into the next action.
///
/// Unlike [`close_error`] (used while connecting, before a session exists), this
/// distinguishes a client-requested clean shutdown from a server-sent
/// `1000`/`1001`: only the former ends the client.
///
/// Reference: `docs/opcodes-and-status-codes.md` (Gateway Close Event Codes),
/// `docs/gateway.md` (Initiating a Disconnect, Handling a Disconnect).
fn close_outcome(
    code: u16,
    reason: &str,
    session_present: bool,
    established: bool,
    shutdown_requested: bool,
) -> Result<Outcome, Error> {
    match close::classify(code) {
        CloseAction::Fatal => Err(Error::Gateway(close::fatal_error(code, reason))),
        CloseAction::Normal if shutdown_requested => {
            tracing::debug!(code, %reason, "the gateway connection closed during shutdown");
            Ok(Outcome::Shutdown)
        }
        CloseAction::Normal => {
            tracing::warn!(
                code,
                %reason,
                "the gateway closed the connection cleanly without a shutdown request; reconnecting"
            );
            Ok(Outcome::Reconnect {
                resume: session_present,
                delay: None,
                reset_backoff: established,
            })
        }
        CloseAction::Reconnect { resume } => {
            tracing::debug!(code, %reason, resume, "the gateway connection closed; reconnecting");
            Ok(Outcome::Reconnect {
                resume: resume && session_present,
                delay: None,
                reset_backoff: established,
            })
        }
    }
}

/// Converts a close frame into a close code and reason.
///
/// A close with no frame carries no status code; RFC 6455 assigns `1005` to
/// that case, which this client treats as a reconnectable disconnect.
fn close_parts(frame: Option<CloseFrame>) -> (u16, String) {
    match frame {
        Some(frame) => (u16::from(frame.code), frame.reason.as_str().to_owned()),
        None => (1005, String::new()),
    }
}

/// Builds the error for a server-initiated close, marking fatal codes.
fn close_error(code: u16, reason: String) -> Error {
    match close::classify(code) {
        CloseAction::Fatal => Error::Gateway(close::fatal_error(code, &reason)),
        CloseAction::Normal | CloseAction::Reconnect { .. } => {
            Error::Gateway(GatewayError::Closed {
                code,
                reason,
                fatal: false,
            })
        }
    }
}

/// Sends one JSON payload over the websocket.
async fn send_json(socket: &mut GatewaySocket, payload: &Value) -> Result<(), Error> {
    let text = serde_json::to_string(payload).map_err(Error::from)?;

    socket.send(Message::text(text)).await.map_err(|error| {
        Error::Gateway(GatewayError::Transport(format!(
            "could not send a gateway payload: {error}"
        )))
    })
}

/// Sends a close frame with the given code and reason.
async fn close_with(socket: &mut GatewaySocket, code: u16, reason: &'static str) {
    let frame = Message::Close(Some(CloseFrame {
        code: CloseCode::from(code),
        reason: reason.into(),
    }));

    if let Err(error) = socket.send(frame).await {
        tracing::debug!(%error, "could not send the gateway close frame");
    }
}

/// Closes the connection cleanly with code `1000` (normal closure).
///
/// The reference notes that closing with `1000` or `1001` invalidates the
/// session, which is the right thing to do on shutdown.
///
/// Reference: `docs/gateway.md` (Initiating a Disconnect).
async fn close_cleanly(socket: &mut GatewaySocket) {
    close_with(socket, 1000, "shutting down").await;
}

/// What the heartbeat timer should do when it fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeartbeatDecision {
    /// Send a heartbeat.
    Send,
    /// The previous heartbeat was never acknowledged, so the connection is
    /// zombied and must be reconnected.
    Zombie,
}

/// Decides whether to heartbeat or reconnect when the heartbeat timer fires.
///
/// Reference: `docs/gateway.md` (Sending Heartbeats — a missing ACK means the
/// connection is "zombied").
const fn heartbeat_decision(awaiting_ack: bool) -> HeartbeatDecision {
    if awaiting_ack {
        HeartbeatDecision::Zombie
    } else {
        HeartbeatDecision::Send
    }
}

/// Creates the heartbeat timer: the first beat after `heartbeat_interval *
/// jitter`, then one beat every `heartbeat_interval`.
///
/// Reference: `docs/gateway.md` (Sending Heartbeats — Heartbeat Interval).
fn heartbeat_timer(interval: Duration) -> tokio::time::Interval {
    let first_heartbeat = initial_heartbeat_delay(interval, jitter_from_seed(random_seed()));
    let mut heartbeat = tokio::time::interval_at(Instant::now() + first_heartbeat, interval);
    heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);

    heartbeat
}

/// The denominator of the fixed-point jitter fraction: one million parts.
///
/// Expressing the documented `0..=1` jitter as integer millionths keeps the
/// conversion back to a [`Duration`] exact and avoids floating point.
const JITTER_SCALE: u64 = 1_000_000;

/// Computes the delay before the first heartbeat: `interval * jitter`, with the
/// jitter clamped to the documented `0..=1` range.
///
/// Reference: `docs/gateway.md` (Sending Heartbeats — Heartbeat Interval).
fn initial_heartbeat_delay(interval: Duration, jitter_ppm: u64) -> Duration {
    let jitter_ppm = jitter_ppm.min(JITTER_SCALE);
    let millis = interval.as_millis() * u128::from(jitter_ppm) / u128::from(JITTER_SCALE);

    Duration::from_millis(u64::try_from(millis).unwrap_or(u64::MAX))
}

/// How long to wait before re-identifying after Invalid Session (`d: false`):
/// a random value between 1 and 5 seconds.
///
/// Reference: `docs/gateway.md` (Resuming — an invalid session is followed by a
/// new Identify).
fn reidentify_delay(seed: u64) -> Duration {
    let extra_millis = 4_000u128 * u128::from(jitter_from_seed(seed)) / u128::from(JITTER_SCALE);
    let extra = Duration::from_millis(u64::try_from(extra_millis).unwrap_or(u64::MAX));

    Duration::from_secs(1) + extra
}

/// Expands a 64-bit seed into a uniform jitter fraction in millionths.
///
/// Returns a value in `0..=JITTER_SCALE`, which is the documented jitter range
/// `0..=1` in fixed point. Uses the `SplitMix64` finalizer so successive
/// timestamps (and the counter that [`random_seed`] mixes in) produce
/// well-distributed jitter.
fn jitter_from_seed(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;

    // Scale the full u64 range onto `0..=JITTER_SCALE`. The u128 intermediate
    // cannot overflow (2^64 * 10^6 < 2^128) and the quotient always fits in a
    // u64; the fallback only guards the checked conversion.
    let scaled = u128::from(z) * u128::from(JITTER_SCALE) / u128::from(u64::MAX);

    u64::try_from(scaled).unwrap_or(JITTER_SCALE)
}

/// Produces a seed for the heartbeat/Invalid Session jitter.
///
/// The crate deliberately has no random-number dependency; the clock, a
/// process-unique counter and the process id are enough to stop clients
/// reconnecting in lockstep.
fn random_seed() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            // Saturate rather than truncate: a clock more than ~584 years after the
            // Unix epoch would not fit in a `u64`, and the seed only needs to vary.
            u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
        });

    nanos ^ count.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (u64::from(std::process::id()) << 32)
}

/// The exponential reconnect backoff: ~1 s, doubling, capped at 60 s.
///
/// Reference: `docs/gateway.md` (Handling a Disconnect — reconnects should be
/// retried with a backoff rather than in a tight loop).
struct ReconnectBackoff {
    attempt: u32,
}

impl ReconnectBackoff {
    /// The shortest delay between reconnect attempts.
    const INITIAL: Duration = Duration::from_secs(1);
    /// The longest delay between reconnect attempts.
    const MAX: Duration = Duration::from_secs(60);

    /// Creates a backoff at its initial delay.
    const fn new() -> Self {
        Self { attempt: 0 }
    }

    /// Resets the backoff after a successful session.
    const fn reset(&mut self) {
        self.attempt = 0;
    }

    /// Returns the delay for the next attempt and advances the schedule.
    fn next_delay(&mut self) -> Duration {
        let delay = Self::INITIAL.saturating_mul(1_u32 << self.attempt.min(6));
        self.attempt = self.attempt.saturating_add(1);

        delay.min(Self::MAX)
    }
}

/// Chooses how long to wait before the next connection attempt.
///
/// A disconnect of an established session is reconnected and resumed
/// immediately; a new session (no session to resume) and repeated consecutive
/// failures use the exponential backoff. A protocol-prescribed delay (Invalid
/// Session) always wins.
///
/// Reference: `docs/gateway.md` (Handling a Disconnect, Resuming),
/// `docs/opcodes-and-status-codes.md` (Gateway Opcodes — Reconnect).
struct ReconnectSchedule {
    backoff: ReconnectBackoff,
    /// True for the first attempt after an established session ended, so the
    /// immediate Resume is only immediate once: failures then back off again.
    immediate_resume: bool,
}

impl ReconnectSchedule {
    /// Creates a schedule for a shard that has not had a session yet.
    const fn new() -> Self {
        Self {
            backoff: ReconnectBackoff::new(),
            immediate_resume: false,
        }
    }

    /// Records that the connection reached Ready/Resumed.
    ///
    /// The next reconnect can resume immediately, and the failure backoff
    /// restarts from its initial delay.
    const fn session_established(&mut self) {
        self.backoff.reset();
        self.immediate_resume = true;
    }

    /// Returns the delay before the next connection attempt.
    fn next_delay(&mut self, resume: bool, prescribed: Option<Duration>) -> Duration {
        if let Some(delay) = prescribed {
            self.immediate_resume = false;
            return delay;
        }

        if resume && self.immediate_resume {
            self.immediate_resume = false;
            return Duration::ZERO;
        }

        self.immediate_resume = false;
        self.backoff.next_delay()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_url_matches_the_documented_example() {
        assert_eq!(GATEWAY_URL, "wss://gateway.discord.gg/?v=10&encoding=json");
    }

    #[test]
    fn fresh_connections_use_the_cached_gateway_url() {
        assert_eq!(connection_url(None, GATEWAY_URL), GATEWAY_URL);
    }

    #[test]
    fn reconnects_use_the_resume_gateway_url_with_the_same_query() {
        let session = ResumeState {
            session_id: "session".to_owned(),
            resume_url: "wss://gateway-us-east1-b.discord.gg".to_owned(),
            seq: Some(42),
        };

        assert_eq!(
            connection_url(Some(&session), GATEWAY_URL),
            "wss://gateway-us-east1-b.discord.gg/?v=10&encoding=json"
        );

        let with_slash = ResumeState {
            session_id: "session".to_owned(),
            resume_url: "wss://gateway-us-east1-b.discord.gg/".to_owned(),
            seq: None,
        };
        assert_eq!(
            connection_url(Some(&with_slash), GATEWAY_URL),
            "wss://gateway-us-east1-b.discord.gg/?v=10&encoding=json"
        );

        let with_query = ResumeState {
            session_id: "session".to_owned(),
            resume_url: "wss://gateway-us-east1-b.discord.gg/?v=10&encoding=json".to_owned(),
            seq: None,
        };
        assert_eq!(
            connection_url(Some(&with_query), GATEWAY_URL),
            "wss://gateway-us-east1-b.discord.gg/?v=10&encoding=json"
        );
    }

    #[test]
    fn heartbeat_jitter_delay_is_bounded_by_the_interval() {
        let interval = Duration::from_secs(45);

        assert_eq!(initial_heartbeat_delay(interval, 0), Duration::ZERO);
        assert_eq!(initial_heartbeat_delay(interval, JITTER_SCALE), interval);
        assert_eq!(
            initial_heartbeat_delay(interval, JITTER_SCALE / 2),
            Duration::from_millis(22_500)
        );
        // Out-of-range jitter is clamped instead of producing a silly delay.
        assert_eq!(
            initial_heartbeat_delay(interval, JITTER_SCALE * 2),
            interval
        );
        assert_eq!(initial_heartbeat_delay(interval, u64::MAX), interval);
    }

    #[test]
    fn jitter_from_seed_stays_in_the_documented_range() {
        for seed in [0, 1, 2, 42, u64::MAX, u64::MAX / 3, 0xDEAD_BEEF_CAFE_BABE] {
            let jitter = jitter_from_seed(seed);
            assert!(
                jitter <= JITTER_SCALE,
                "seed {seed} -> {jitter} millionths is above the documented 1.0"
            );
        }
    }

    #[test]
    fn reidentify_delay_is_between_one_and_five_seconds() {
        for seed in [0, 1, 2, 42, u64::MAX, 0xDEAD_BEEF_CAFE_BABE] {
            let delay = reidentify_delay(seed);
            assert!(
                delay >= Duration::from_secs(1) && delay <= Duration::from_secs(5),
                "seed {seed} -> {delay:?}"
            );
        }
    }

    #[test]
    fn heartbeat_timer_sends_until_an_ack_is_missing() {
        assert_eq!(heartbeat_decision(false), HeartbeatDecision::Send);
        assert_eq!(heartbeat_decision(true), HeartbeatDecision::Zombie);
    }

    #[test]
    fn reconnect_backoff_doubles_from_one_second_and_caps_at_sixty() {
        let mut backoff = ReconnectBackoff::new();
        let waits: Vec<u64> = (0..9).map(|_| backoff.next_delay().as_secs()).collect();

        assert_eq!(waits, vec![1, 2, 4, 8, 16, 32, 60, 60, 60]);

        backoff.reset();
        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    }

    #[test]
    fn the_first_reconnect_of_an_established_session_is_immediate() {
        let mut schedule = ReconnectSchedule::new();

        // A session reached Ready: resume immediately, not after the backoff.
        schedule.session_established();
        assert_eq!(schedule.next_delay(true, None), Duration::ZERO);

        // If that resume attempt fails, consecutive failures back off.
        assert_eq!(schedule.next_delay(true, None), Duration::from_secs(1));
        assert_eq!(schedule.next_delay(true, None), Duration::from_secs(2));
    }

    #[test]
    fn re_identifying_after_a_disconnect_uses_the_backoff() {
        let mut schedule = ReconnectSchedule::new();
        schedule.session_established();

        // No session to resume (for example close code 4007/4009): re-identify
        // after the exponential backoff, never immediately.
        assert_eq!(schedule.next_delay(false, None), Duration::from_secs(1));
        assert_eq!(schedule.next_delay(false, None), Duration::from_secs(2));
    }

    #[test]
    fn a_protocol_prescribed_delay_wins_and_does_not_consume_the_backoff() {
        let mut schedule = ReconnectSchedule::new();
        schedule.session_established();

        // Invalid Session (`d: false`) prescribes 1-5 s before re-identifying.
        assert_eq!(
            schedule.next_delay(false, Some(Duration::from_secs(3))),
            Duration::from_secs(3)
        );
        assert_eq!(schedule.next_delay(false, None), Duration::from_secs(1));
    }

    #[test]
    fn a_new_session_after_ready_still_backs_off_from_the_start() {
        let mut schedule = ReconnectSchedule::new();
        schedule.session_established();
        // The resume fails and the gateway asks for a new Identify.
        assert_eq!(schedule.next_delay(true, None), Duration::ZERO);
        assert_eq!(schedule.next_delay(false, None), Duration::from_secs(1));
    }

    #[test]
    fn fatal_close_becomes_a_fatal_gateway_error() {
        let error = close_error(4004, "Authentication failed.".to_owned());

        match error {
            Error::Gateway(GatewayError::Closed { code, fatal, .. }) => {
                assert_eq!(code, 4004);
                assert!(fatal);
            }
            other => panic!("expected a fatal closed error, got {other:?}"),
        }
    }

    #[test]
    fn reconnectable_close_is_not_marked_fatal() {
        let error = close_error(4000, "Unknown error".to_owned());

        match error {
            Error::Gateway(GatewayError::Closed { code, fatal, .. }) => {
                assert_eq!(code, 4000);
                assert!(!fatal);
            }
            other => panic!("expected a reconnectable closed error, got {other:?}"),
        }
    }

    #[test]
    fn a_received_clean_close_reconnects_unless_a_shutdown_was_requested() {
        // Client-initiated clean close (shutdown already requested): stop.
        match close_outcome(1000, "shutting down", true, true, true) {
            Ok(Outcome::Shutdown) => {}
            _ => panic!("a clean close after shutdown_all must end the shard"),
        }

        // Server-sent 1000 with a session: reconnect and resume.
        match close_outcome(1000, "server restart", true, true, false) {
            Ok(Outcome::Reconnect {
                resume,
                delay,
                reset_backoff,
            }) => {
                assert!(resume);
                assert!(delay.is_none());
                assert!(reset_backoff);
            }
            _ => panic!("a server-sent clean close must reconnect"),
        }

        // Server-sent 1001 without a session: reconnect with a fresh Identify.
        match close_outcome(1001, "going away", false, false, false) {
            Ok(Outcome::Reconnect {
                resume,
                delay,
                reset_backoff,
            }) => {
                assert!(!resume);
                assert!(delay.is_none());
                assert!(!reset_backoff);
            }
            _ => panic!("a server-sent 1001 must reconnect"),
        }
    }

    #[test]
    fn a_fatal_close_ends_the_shard_even_after_ready() {
        let result = close_outcome(4014, "Disallowed intent(s).", true, true, false);

        match result {
            Err(Error::Gateway(GatewayError::Closed { code, fatal, .. })) => {
                assert_eq!(code, 4014);
                assert!(fatal);
            }
            _ => panic!("a fatal close must return an error"),
        }
    }

    #[test]
    fn close_without_a_frame_is_treated_as_an_unknown_status_code() {
        assert_eq!(close_parts(None), (1005, String::new()));
    }

    #[test]
    fn error_or_reconnect_returns_fatal_errors_and_retries_the_rest() {
        let fatal = Error::Gateway(close::fatal_error(4014, "Disallowed intent(s)."));
        assert!(matches!(
            reconnect_after_error(fatal, false, false),
            Err(Error::Gateway(GatewayError::Closed { fatal: true, .. }))
        ));

        let transport = Error::Gateway(GatewayError::Transport("boom".to_owned()));
        match reconnect_after_error(transport, false, false) {
            Ok(Outcome::Reconnect {
                resume,
                delay,
                reset_backoff,
            }) => {
                assert!(!resume);
                assert!(delay.is_none());
                assert!(!reset_backoff);
            }
            Ok(Outcome::Shutdown) => panic!("must not shut down on a transport error"),
            Err(error) => panic!("must not fail on a transport error: {error}"),
        }
    }

    // ---------------------------------------------------------------------
    // Wire tests against an in-process mock gateway (no live Discord).
    // ---------------------------------------------------------------------

    use std::sync::atomic::AtomicBool;

    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio::sync::RwLock;
    use tokio_tungstenite::accept_async;

    use crate::cache::Cache;
    use crate::client::{Data, ShardHandle};
    use crate::http::client::HttpClient;
    use crate::model::ready::Resumed;

    /// The client end of the mock gateway's websocket.
    type MockSocket = WebSocketStream<TcpStream>;

    /// An in-process websocket server that speaks the gateway protocol.
    struct MockGateway {
        listener: TcpListener,
        url: String,
    }

    impl MockGateway {
        /// Binds an ephemeral port and returns the `ws://` URL to connect to.
        async fn bind() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("the mock gateway binds a local port");
            let port = listener
                .local_addr()
                .expect("the mock gateway has a local address")
                .port();

            Self {
                listener,
                url: format!("ws://127.0.0.1:{port}/?v=10&encoding=json"),
            }
        }

        /// Accepts the next client connection and completes the websocket
        /// handshake.
        async fn accept(&self) -> MockSocket {
            let accepted = tokio::time::timeout(Duration::from_secs(5), self.listener.accept())
                .await
                .expect("the client connects to the mock gateway");
            let (stream, _) = accepted.expect("the mock gateway accepts the connection");

            accept_async(stream)
                .await
                .expect("the websocket handshake succeeds")
        }
    }

    /// Sends one JSON gateway payload to the client.
    async fn send_payload(socket: &mut MockSocket, payload: Value) {
        socket
            .send(Message::text(payload.to_string()))
            .await
            .expect("the mock gateway sends a payload");
    }

    /// Receives one text gateway payload from the client.
    async fn recv_payload(socket: &mut MockSocket) -> Value {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
                .await
                .expect("the client sends a gateway payload");

            match message {
                Some(Ok(Message::Text(text))) => {
                    return serde_json::from_str(&text).expect("the payload is JSON");
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                other => panic!("expected a text payload, got {other:?}"),
            }
        }
    }

    /// Reads frames until the client closes the connection; returns its code.
    async fn recv_close_code(socket: &mut MockSocket) -> Option<u16> {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
                .await
                .expect("the client closes the connection");

            match message {
                Some(Ok(Message::Close(frame))) => {
                    return frame.map(|frame| u16::from(frame.code));
                }
                Some(Ok(
                    Message::Text(_) | Message::Ping(_) | Message::Pong(_) | Message::Binary(_),
                )) => {}
                other => panic!("expected a close frame, got {other:?}"),
            }
        }
    }

    /// Closes the mock gateway's side of the connection with a given code.
    async fn send_close(socket: &mut MockSocket, code: u16, reason: &'static str) {
        socket
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::from(code),
                reason: reason.into(),
            })))
            .await
            .expect("the mock gateway sends a close frame");
    }

    /// A Hello payload with the given heartbeat interval.
    fn hello(heartbeat_interval: i64) -> Value {
        json!({"op": 10, "d": {"heartbeat_interval": heartbeat_interval}})
    }

    /// A documented Ready payload whose resume URL points at the mock gateway.
    fn ready_payload(resume_url: &str) -> Value {
        json!({
            "op": 0,
            "s": 1,
            "t": "READY",
            "d": {
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
                "guilds": [{"id": "10", "unavailable": true}],
                "session_id": "test-session",
                "resume_gateway_url": resume_url,
                "application": {"id": "3", "flags": 0},
            },
        })
    }

    /// Records the high-level events a handler receives.
    struct RecordingHandler {
        events: mpsc::UnboundedSender<&'static str>,
    }

    impl EventHandler for RecordingHandler {
        async fn on_ready(&self, _ctx: Context, _ready: Ready) {
            let _ = self.events.send("ready");
        }

        async fn on_resumed(&self, _ctx: Context, _resumed: Resumed) {
            let _ = self.events.send("resumed");
        }
    }

    /// A shard wired to a mock gateway instead of the real one.
    struct TestShard {
        shard: Shard<RecordingHandler>,
        out_rx: mpsc::UnboundedReceiver<String>,
        cache: Arc<Cache>,
        manager: Arc<ShardManager>,
        events: mpsc::UnboundedReceiver<&'static str>,
    }

    /// Builds a [`TestShard`] whose fresh connections go to `url`.
    fn test_shard(url: &str) -> TestShard {
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let handler = Arc::new(RecordingHandler { events: event_tx });
        let cache = Arc::new(Cache::new());
        let ctx = Context::new(
            HttpClient::new("test-token"),
            Arc::clone(&cache),
            Arc::new(RwLock::new(Data::default())),
            ShardHandle::new(out_tx, Arc::new(AtomicBool::new(false))),
        );
        let manager = Arc::new(ShardManager::new());
        let mut shard = Shard::new(
            "test-token".to_owned(),
            GatewayIntents::GUILDS,
            handler,
            ctx,
            Arc::clone(&manager),
        );
        shard.gateway_url = url.to_owned();

        TestShard {
            shard,
            out_rx,
            cache,
            manager,
            events: event_rx,
        }
    }

    /// A spawned shard task plus the shared state a test inspects.
    struct SpawnedShard {
        run: tokio::task::JoinHandle<Result<(), Error>>,
        cache: Arc<Cache>,
        manager: Arc<ShardManager>,
        events: mpsc::UnboundedReceiver<&'static str>,
    }

    /// Spawns a shard and hands back its task plus the shared state.
    fn spawn_shard(test: TestShard) -> SpawnedShard {
        let TestShard {
            shard,
            out_rx,
            cache,
            manager,
            events,
        } = test;

        let run = tokio::spawn(async move { shard.run(out_rx).await });

        SpawnedShard {
            run,
            cache,
            manager,
            events,
        }
    }

    #[tokio::test]
    async fn connects_identifies_and_dispatches_ready() {
        let gateway = MockGateway::bind().await;
        let SpawnedShard {
            run,
            cache,
            manager,
            mut events,
        } = spawn_shard(test_shard(&gateway.url));

        let mut socket = gateway.accept().await;
        send_payload(&mut socket, hello(45_000)).await;

        let identify = recv_payload(&mut socket).await;
        assert_eq!(identify["op"], 2);
        assert_eq!(identify["d"]["token"], "test-token");
        assert_eq!(identify["d"]["intents"], 1);
        assert_eq!(identify["d"]["compress"], false);
        assert_eq!(
            identify["d"]["properties"]["browser"],
            env!("CARGO_PKG_NAME")
        );
        assert_eq!(
            identify["d"]["properties"]["device"],
            env!("CARGO_PKG_NAME")
        );

        send_payload(&mut socket, ready_payload(&gateway.url)).await;
        let ready = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("the handler runs");
        assert_eq!(ready, Some("ready"));
        assert_eq!(cache.current_user().map(|user| user.id.get()), Some(2));
        assert_eq!(cache.application_id().map(crate::id::Id::get), Some(3));

        tokio::time::timeout(Duration::from_secs(5), manager.shutdown_all())
            .await
            .expect("a requested shutdown completes");

        assert_eq!(recv_close_code(&mut socket).await, Some(1000));
        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the shard stops")
            .expect("the shard task does not panic");
        assert!(result.is_ok(), "a clean shutdown returns Ok: {result:?}");
    }

    #[tokio::test]
    async fn fatal_close_code_makes_start_fail() {
        let gateway = MockGateway::bind().await;
        let SpawnedShard { run, .. } = spawn_shard(test_shard(&gateway.url));

        let mut socket = gateway.accept().await;
        send_payload(&mut socket, hello(45_000)).await;
        let identify = recv_payload(&mut socket).await;
        assert_eq!(identify["op"], 2);

        send_close(&mut socket, 4004, "Authentication failed.").await;

        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("a fatal close ends the shard")
            .expect("the shard task does not panic");

        match result {
            Err(Error::Gateway(GatewayError::Closed { code, fatal, .. })) => {
                assert_eq!(code, 4004);
                assert!(fatal);
            }
            other => panic!("expected a fatal gateway error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn reconnect_event_resumes_the_session() {
        let gateway = MockGateway::bind().await;
        let SpawnedShard {
            run,
            manager,
            mut events,
            ..
        } = spawn_shard(test_shard(&gateway.url));

        let mut socket = gateway.accept().await;
        send_payload(&mut socket, hello(45_000)).await;
        let identify = recv_payload(&mut socket).await;
        assert_eq!(identify["op"], 2);
        send_payload(&mut socket, ready_payload(&gateway.url)).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("the handler runs"),
            Some("ready")
        );

        // The server asks for a reconnect; the client must not close the
        // connection (the session must stay resumable) and must send Resume on
        // the next connection.
        send_payload(&mut socket, json!({"op": 7, "d": null})).await;

        let mut resumed_socket = gateway.accept().await;
        send_payload(&mut resumed_socket, hello(45_000)).await;

        let resume = recv_payload(&mut resumed_socket).await;
        assert_eq!(resume["op"], 6);
        assert_eq!(resume["d"]["token"], "test-token");
        assert_eq!(resume["d"]["session_id"], "test-session");
        assert_eq!(resume["d"]["seq"], 1);

        send_payload(
            &mut resumed_socket,
            json!({"op": 0, "s": 2, "t": "RESUMED", "d": {}}),
        )
        .await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("the handler runs"),
            Some("resumed")
        );

        tokio::time::timeout(Duration::from_secs(5), manager.shutdown_all())
            .await
            .expect("a requested shutdown completes");
        assert_eq!(recv_close_code(&mut resumed_socket).await, Some(1000));
        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the shard stops")
            .expect("the shard task does not panic");
        assert!(result.is_ok(), "a clean shutdown returns Ok: {result:?}");
    }

    #[tokio::test]
    async fn unacknowledged_heartbeat_reconnects() {
        let gateway = MockGateway::bind().await;
        let SpawnedShard { run, manager, .. } = spawn_shard(test_shard(&gateway.url));

        let mut socket = gateway.accept().await;
        // A short interval keeps the test fast; no ACKs are ever sent.
        send_payload(&mut socket, hello(50)).await;
        let identify = recv_payload(&mut socket).await;
        assert_eq!(identify["op"], 2);
        send_payload(&mut socket, ready_payload(&gateway.url)).await;

        // The client must notice the missing ACK and close with a code other
        // than 1000/1001 before reconnecting and resuming.
        let code = recv_close_code(&mut socket).await;
        assert_eq!(code, Some(close::ZOMBIE_CLOSE_CODE));

        tokio::time::timeout(Duration::from_secs(5), manager.shutdown_all())
            .await
            .expect("a requested shutdown completes");
        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the shard stops")
            .expect("the shard task does not panic");
        assert!(result.is_ok(), "a clean shutdown returns Ok: {result:?}");
    }

    #[tokio::test]
    async fn server_sent_clean_close_reconnects_and_resumes() {
        let gateway = MockGateway::bind().await;
        let SpawnedShard { run, manager, .. } = spawn_shard(test_shard(&gateway.url));

        let mut socket = gateway.accept().await;
        send_payload(&mut socket, hello(45_000)).await;
        let identify = recv_payload(&mut socket).await;
        assert_eq!(identify["op"], 2);
        send_payload(&mut socket, ready_payload(&gateway.url)).await;

        // A server-sent 1000 is not a client-initiated shutdown: the client must
        // reconnect and resume instead of ending the bot.
        send_close(&mut socket, 1000, "server going away").await;

        let mut resumed_socket = gateway.accept().await;
        send_payload(&mut resumed_socket, hello(45_000)).await;
        let resume = recv_payload(&mut resumed_socket).await;
        assert_eq!(resume["op"], 6);
        assert_eq!(resume["d"]["session_id"], "test-session");

        tokio::time::timeout(Duration::from_secs(5), manager.shutdown_all())
            .await
            .expect("a requested shutdown completes");
        assert_eq!(recv_close_code(&mut resumed_socket).await, Some(1000));
        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the shard stops")
            .expect("the shard task does not panic");
        assert!(result.is_ok(), "a clean shutdown returns Ok: {result:?}");
    }

    #[tokio::test]
    async fn reconnect_before_hello_reconnects_instead_of_waiting_for_the_timeout() {
        let gateway = MockGateway::bind().await;
        let SpawnedShard { run, manager, .. } = spawn_shard(test_shard(&gateway.url));

        let mut socket = gateway.accept().await;
        // `docs/gateway-events.md` (Reconnect): the event "can occur at any
        // point in the gateway connection lifecycle, even before/in place of
        // receiving a Hello event."
        send_payload(&mut socket, json!({"op": 7, "d": null})).await;

        // The client must reconnect (not sit on the 30s Hello timeout). There is
        // no session yet, so the new connection starts with Identify.
        let mut reconnected = gateway.accept().await;
        send_payload(&mut reconnected, hello(45_000)).await;
        let identify = recv_payload(&mut reconnected).await;
        assert_eq!(identify["op"], 2);
        assert_eq!(identify["d"]["token"], "test-token");

        tokio::time::timeout(Duration::from_secs(5), manager.shutdown_all())
            .await
            .expect("a requested shutdown completes");
        assert_eq!(recv_close_code(&mut reconnected).await, Some(1000));
        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the shard stops")
            .expect("the shard task does not panic");
        assert!(result.is_ok(), "a clean shutdown returns Ok: {result:?}");
    }
}
