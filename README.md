# discord-bot-template

**A single-guild Discord moderation bot built on this repository's own from-scratch
[`discord`](crates/discord) crate — not on a third-party Discord library.**

[![Lint, Build and Test](https://github.com/gage-lodba/discord-bot-template/actions/workflows/Lint_Build_Test.yml/badge.svg?branch=main)](https://github.com/gage-lodba/discord-bot-template/actions/workflows/Lint_Build_Test.yml)
[![Secret scan](https://github.com/gage-lodba/discord-bot-template/actions/workflows/secret-scan.yml/badge.svg?branch=main)](https://github.com/gage-lodba/discord-bot-template/actions/workflows/secret-scan.yml)
![Rust 1.98](https://img.shields.io/badge/rust-1.98-000000?logo=rust&logoColor=white)

[Overview](#overview) · [Features](#features) · [The Discord crate](#the-discord-crate) · [Quick start](#quick-start) · [Commands](#commands) · [Configuration](#configuration) · [Building](#building) · [Development](#development) · [Security](#security)

## Overview

`discord-bot-template` is a small, single-guild moderation bot. It handles bans,
kicks, role management, a rules/invite dashboard, and join/leave announcements,
and it is built entirely on the `discord` crate that lives in this workspace.
That crate is a **from-scratch** implementation of the Discord v10 API written
against the [official Discord API
reference](https://discord.com/developers/docs/): no third-party Discord library
is linked, wrapped or copied.

CI builds and tests on Windows and Linux x64; the release profile ships a small
binary with no native TLS dependency.

Because the Discord layer is part of the workspace, the repository is also a
starting point for your own bot: the crate grows with the bot instead of being
replaced by a dependency.

## Features

- **Slash commands** — `/ping`, `/info`, `/say`, `/ban`, `/kick`, `/role` and
  `/dashboard`, with fail-closed permission checks.
- **Live role management** — `/role` autocompletes from the server's roles,
  validates the role hierarchy before changing anything, and the ephemeral result
  carries an **Undo** button.
- **Every interaction type routed** — commands, buttons/selects, autocomplete and
  modal submits; unhandled component ids are answered with an ephemeral message
  instead of Discord's "This interaction failed".
- **Complete interactions API** — all five interaction types, every documented
  callback response, resolved data, the full endpoint set (original response and
  followups), polls, multipart file uploads, and typed models/builders for every
  documented component type, including `IS_COMPONENTS_V2` layout components.
- **Safety first** — no `unwrap`/`panic` on remote data, token-redacting `Debug`
  implementations, and all role changes carry an audit-log reason.
- **Strict quality gates** — `clippy` (`all` + `pedantic`) with `-D warnings` and
  no lint allowances (every reported lint is fixed in code, never suppressed),
  warning-free rustdoc, unit/integration/doctests and a full-history secret scan.

## The `discord` crate

[`crates/discord`](crates/discord) implements the parts of the Discord v10 API the
bot needs, plus a complete interactions implementation. Nothing in it is copied
from, or wraps, an existing Discord library.

| Layer | Module | What it does |
| --- | --- | --- |
| Gateway | `discord::gateway` | WebSocket connection: hello/heartbeat, identify, resume and reconnect, close-code handling, event dispatch (including automatic PONG for PING interactions) |
| REST | `discord::http` | Authenticated HTTP client with Discord's per-route and global rate limits, retries, and the endpoints the bot and interactions need |
| Models | `discord::model` | Guild, channel, member, role, message, emoji, audit log, command, all five interaction types, and typed component objects for every documented component type |
| Builders | `discord::builder` | Application commands, messages with polls/files/embeds, every interaction response (reply/defer/update/autocomplete/modal), all components (including `IS_COMPONENTS_V2` layout components), and presence |
| Cache | `discord::cache` | The guild, its channels/threads/roles and the current user |
| Client | `discord::client` | `Client`, `Context`, the typed data map and the `EventHandler` trait |

The crate covers the v10 surface the bot uses **plus a complete interactions
implementation**: all five interaction types, every documented interaction
callback response, resolved data, the full interaction endpoint set (original
response + followups), polls, file uploads (multipart), and typed models/builders
for **every** documented component type including the `IS_COMPONENTS_V2`
layout/content components. Adding a new endpoint means adding one HTTP method, its
model and its builder, not reworking the gateway.

The interface between layers and the code layout contract are documented in
[`crates/discord/CONTRACT.md`](crates/discord/CONTRACT.md).

## Quick start

### Prerequisites

- **Rust 1.98** — [`rust-toolchain.toml`](rust-toolchain.toml) pins the exact
  toolchain; running `cargo`/`rustup` from the repository installs it
  automatically.
- A **Discord application** with a bot token, and the privileged intents below
  enabled in the [Developer Portal](https://discord.com/developers/applications)
  (*Bot → Privileged Gateway Intents*).
- The **ids** of the guild the bot runs in and the channel it announces
  moderation events in, plus an optional owner user id, application emoji id and
  invite URL — all listed under [Configuration](#configuration).

### Run the bot

All configuration is read from the process environment. No deployment ids are
baked into the binary and there is no fallback: a missing required variable is a
start-up error, not a silent default.

Three variables are required — `DISCORD_TOKEN`, `DISCORD_GUILD_ID` and
`DISCORD_LOG_CHANNEL_ID` — and three are optional: `DISCORD_OWNER_ID`,
`DISCORD_GITHUB_EMOJI_ID` and `DISCORD_INVITE_URL`. The [Configuration](#configuration)
table documents what each one changes.

The token is never hard-coded, never compiled into the binary and never loaded
from a `.env` file: the bot reads only real environment variables.

```bash
export DISCORD_TOKEN="<your bot token>"
export DISCORD_GUILD_ID="<your guild id>"
export DISCORD_LOG_CHANNEL_ID="<your log channel id>"
cargo run --release
```

```powershell
$env:DISCORD_TOKEN = "<your bot token>"
$env:DISCORD_GUILD_ID = "<your guild id>"
$env:DISCORD_LOG_CHANNEL_ID = "<your log channel id>"
cargo run --release
```

The optional variables are exported the same way when the deployment uses them.
[`.env.example`](.env.example) lists all six names as a reference.

If a required variable is missing or empty, or an id is non-numeric or zero, the
bot logs an error naming the offending variable and exits with status 1; an
invalid token fails at start-up in the same way. Invite the bot with the `bot`
and `applications.commands` scopes; guild commands are registered in
`DISCORD_GUILD_ID` on every start.

## Commands

| Command | Permission | What it does |
| --- | --- | --- |
| `/ping` | everyone | Ephemeral "Pong!" latency check |
| `/info` | everyone | Bot info embed with a GitHub link button |
| `/say message:` | Manage Messages | Posts the given text as the bot |
| `/ban target: [duration:] [reason:]` | Ban Members | Bans a member (optional 0-7 days of message cleanup) |
| `/kick target: [reason:]` | Kick Members | Kicks a member |
| `/role target: role: execute:` | Manage Roles | Adds or removes a role: `role` autocompletes from the server's roles and `execute` offers fixed **Add**/**Remove** choices; the ephemeral result has an **Undo** button |
| `/dashboard` | Administrator | Posts the rules and invite embeds |

All commands are guild-scoped and re-registered on every start in
`DISCORD_GUILD_ID`. The bot routes every interaction type: slash commands,
buttons/selects, autocomplete and modal submits (unhandled component ids are
answered with an ephemeral message), and PING interactions are acknowledged by
the crate's gateway dispatch.

`/dashboard` posts the rules embed and, only when `DISCORD_INVITE_URL` is set, a
second invite-link embed; without the variable the invite embed is omitted.
`/info` shows the creator mention when `DISCORD_OWNER_ID` is set and the button
icon when `DISCORD_GITHUB_EMOJI_ID` is set.

### Announcements and the leave message

Join, leave, kick, ban and unban announcements are posted to
`DISCORD_LOG_CHANNEL_ID`.

| Action | Announcement |
| --- | --- |
| Join | "Has joined the server." |
| Voluntary leave | "Has left the server." |
| Kick | "Kick" embed (no leave message) |
| Ban | "Ban" embed (no leave message) |
| Unban | "Unbanned" embed |

<details>
<summary>Why genuine leaves appear about 1.5 seconds late</summary>

Discord fires `GUILD_MEMBER_REMOVE` for a voluntary leave, a kick **and** a ban,
and the event does not say which. The audit-log entry that does is written by
Discord server-side and arrives *after* the removal event, so the leave
announcement is held for 1.5 seconds while the audit-log entry is recorded. If it
turns out the member was kicked or banned, the leave message is dropped because
their own embed already covers it.

Consequences:

- **Genuine leaves are delayed by about 1.5 seconds.** That is the cost of being
  able to tell them apart. Tune `RECONCILE_DELAY` in `src/moderation.rs`.
- **The bot needs the View Audit Log permission.** Without it Discord sends no
  audit entries, so kicks and bans cannot be distinguished and a leave message
  will also be posted. A missing permission shows up as duplicate announcements
  rather than an error.

</details>

## Configuration

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `DISCORD_TOKEN` | **yes** | — | Bot token used for the gateway and REST calls |
| `DISCORD_GUILD_ID` | **yes** | — | Guild the slash commands are registered in; must match the guild the bot actually runs in |
| `DISCORD_LOG_CHANNEL_ID` | **yes** | — | Channel join/leave/kick/ban events are announced in; the bot needs View Audit Log there to tell kicks and bans from genuine leaves |
| `DISCORD_OWNER_ID` | no | — | User id shown as the creator mention in `/info`; the mention line is omitted when unset |
| `DISCORD_GITHUB_EMOJI_ID` | no | — | Application emoji id used as the `/info` link-button icon; no icon is shown when unset |
| `DISCORD_INVITE_URL` | no | — | Invite link added as an embed on `/dashboard`; the embed is omitted when unset |

The three required variables have no defaults: every deployment supplies its own
token, guild id and log-channel id, so nothing deployment-specific is compiled
into the binary. A missing or empty required variable, or an id that is
non-numeric or zero, fails start-up with an error naming the variable.

The optional variables keep no fallback values: when unset or blank, the feature
they configure is omitted rather than pointed at some other deployment.
`DISCORD_GITHUB_EMOJI_ID` must name one of this application's own application
emojis (application emojis can only be used by the application that owns them);
if the lookup fails the icon is dropped and `/info` still answers.
`DISCORD_INVITE_URL` must be an `http`/`https` URL; any other value fails
start-up with an error naming the variable.

`DISCORD_GUILD_ID` must match the guild the bot actually runs in: `/ban` and
`/kick` check the invoker's permissions against the guild the command was used
in and then act on that same guild.

### Required gateway intents

The privileged intents **Message Content** and **Server Members** must be enabled
for the application in the Developer Portal. Without them Discord closes the
gateway with code 4014 and the bot will not start.

The bot requests only the intents it uses: `GUILDS`, `GUILD_MESSAGES`,
`MESSAGE_CONTENT`, `GUILD_MEMBERS` and `GUILD_MODERATION`. `GUILDS` is required:
the guild cache is populated from `GUILD_CREATE`, so without it the cache stays
empty and every permission check fails closed.

### Shutdown

The bot closes its gateway connection cleanly on `Ctrl+C` and, on Unix, on
`SIGTERM` (so `docker stop` and systemd do not drop the connection). It waits up
to 10 seconds for the shard to stop before exiting anyway. The close uses code
1000/1001, which tells Discord the session is over rather than dropped.

## Building

```bash
cargo build --release
```

Release builds use thin LTO and strip debug info (see `[profile.release]` in
[`Cargo.toml`](Cargo.toml)), and the `discord` crate uses rustls instead of
OpenSSL, so the binary has no native TLS library to install.

CI builds and tests on Windows and Linux x64, and drafts a GitHub release that
attaches both binaries whenever the `Cargo.toml` version changes.

## Development

### Quality gates

Run the same checks CI runs:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --release --all-targets --locked -- -D warnings
$env:RUSTDOCFLAGS = "-D warnings"; cargo doc --workspace --no-deps --locked
cargo test --workspace --locked
cargo build --release --locked
```

On bash, set the env var inline instead:
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`.

### Project layout

```text
.
├── src/                    bot binary: commands, events, moderation, config, error handling
├── crates/discord/         the from-scratch Discord v10 crate
│   ├── src/                gateway, http, model, builder, cache, client
│   ├── tests/public_api.rs external public-API tests
│   └── CONTRACT.md         layer interface and code-layout contract
├── .github/workflows/      Lint/Build/Test and full-history secret scan
├── .env.example            environment-variable reference (not loaded at runtime)
├── Cargo.toml              workspace, dependency versions and lint policy
├── rust-toolchain.toml     pinned compiler
└── LICENSE                 MIT
```

The Discord API documentation is **not vendored in this repository**. The
specification is the [official reference](https://discord.com/developers/docs/)
(the machine-readable index is
<https://discord.com/developers/docs/llms.txt>); code comments cite pages as
`docs/<page>.md`, so keeping a local, gitignored copy under `docs/` makes those
citations navigable. Never commit that copy: the upstream pages contain fake
example credentials that a secret scanner cannot distinguish from real leaks.

### Quality policy

- **Lints are configuration, not CI flags.** Levels live in `[workspace.lints]`
  in `Cargo.toml`, inherited by both crates, so a local `cargo clippy` reports
  exactly what CI rejects: clippy `all` + `pedantic`, `missing_docs`,
  `missing_debug_implementations`, `unreachable_pub`, `unsafe_code = "forbid"`,
  and rustdoc's broken/private/redundant intra-doc link lints. No lint is set to
  `allow`, and code carries no `#[allow]`/`#[expect]` suppressions: a reported
  lint is fixed by tightening the code (checked conversions, smaller functions,
  faithful wire modelling) rather than silenced.
- **Documentation is part of the build.** Public items need doc comments and
  `cargo doc` must stay warning-free; every item cites the reference page that
  specifies it.
- **Version metadata has one home.** Package version, edition and `rust-version`
  live in `[workspace.package]`; third-party dependency versions live in
  `[workspace.dependencies]`, and members only opt in and add features.
- **MSRV is explicit.** `rust-version` in `Cargo.toml` matches the pinned
  `rust-toolchain.toml`, so the tested compiler is the declared minimum.
- **Errors are values.** The crates return `Result` and never `unwrap`/`panic` on
  data from Discord; `Debug` implementations for client types redact the token.
- **Behaviour is tested at both levels.** Unit tests sit next to each module and
  cover wire formats, the permission hierarchy, rate-limit decisions and gateway
  reconnect logic; `crates/discord/tests/public_api.rs` is compiled as an external
  crate to lock down the public API from a downstream user's point of view.

## Security

- The bot token is read from `DISCORD_TOKEN` at start-up. It is never hard-coded,
  never compiled into the binary and redacted from `Debug` output.
- No guild, channel, owner or emoji ids are compiled into the binary either: every
  deployment supplies its own through the environment, so the public source
  contains no private identifiers.
- Every role change is sent with an `X-Audit-Log-Reason`, so moderation actions
  are attributable in the server's audit log.
- CI runs a **full-history secret scan** ([gitleaks](https://github.com/gitleaks/gitleaks))
  on every push to `main` and every pull request, with no baseline; the only
  allowlist entry keeps the committed `Cargo.lock` content out of the matching,
  so a red result means a *new* secret. The Discord API reference copy is
  deliberately kept out of git because its example credentials would trip that
  scan.
- `Cargo.lock` is committed because this is a binary workspace, so builds are
  reproducible and CI cannot silently pick up a different dependency version.

## License

MIT — see [LICENSE](LICENSE).
