# `discord` crate — interface contract

This crate is a **from-scratch** implementation of the Discord v10 API. It is not
a wrapper around, and must not copy code or API shape from, any existing Rust
Discord library.

The specification is the [official Discord API reference](https://discord.com/developers/docs/)
(and its machine-readable index at `https://discord.com/developers/docs/llms.txt`).
Every public item should cite the page that defines it, e.g. `docs/gateway-events.md`.
Those citations name the authoritative upstream pages; the reference copy itself
is not vendored in this template, because its examples contain fake credentials
that trip secret scanners.

Ground rules:

- Keep changes scoped to the layer you are working in.
- Keep `cargo check -p discord` green after each edit batch.
- `cargo fmt --all` before handing work back. CI runs
  `cargo clippy --workspace --release --all-targets --locked -- -D warnings`, so
  no warnings are acceptable.
- Add unit tests next to the code you write. Serde shapes must be tested against
  JSON that matches the documented wire format.
- No `unwrap()`/`expect()`/panics on data received from Discord: a malformed or
  unexpected payload must produce an `Error`, never abort the process.
- Every public type/function gets a doc comment; resource limits from the docs
  (2000-char content, 10 embeds, 25 options, 5 buttons per row, ...) are enforced
  where the builder can do it cheaply.

## Dependency direction

The layers push requests downward only; never reverse it:

```
error, id, timestamp, permissions, intents
        │
      model        (uses id/timestamp/permissions/error)
        │
  builder          (uses model + id/timestamp/permissions)
   ┌────┴─────┐
  http      gateway      (both use model + builder + error)
   └────┬─────┘
     client, cache       (use everything)
```

## Already implemented foundation (use it, don't duplicate it)

- `crate::id` — `Id<Kind>` snowflake; serialises as a decimal JSON string and
  deserialises from string or integer; rejects zero. Aliases:
  `ApplicationId`, `AttachmentId`, `AuditLogEntryId`, `ChannelId`, `CommandId`,
  `EmojiId`, `GenericId`, `GuildId`, `InteractionId`, `MessageId`, `RoleId`,
  `UserId`, `WebhookId`. `Id::new(u64)` panics on zero; `.get()` returns `u64`;
  `.created_at()` returns `Timestamp`.
- `crate::timestamp::Timestamp` — Unix milliseconds internally; `Timestamp::now()`,
  `from_unix_millis`, `from_unix_seconds`, `unix_millis`, `unix_seconds`;
  `Display`/`Serialize` emit ISO8601 UTC (`2026-10-22T13:20:32.123+00:00`);
  `Deserialize` accepts ISO8601 (`Z`, `+HH:MM`, fractional seconds).
- `crate::permissions::Permissions` — documented bit flags (all of
  `docs/permissions.md`), `contains`/`intersects`/`union`/`intersection`/
  `difference`/`insert`/`remove`/`is_empty`/`is_administrator`/`from_bits_truncate`/
  `bits`; serialises as a decimal string, deserialises from string or number.
- `crate::intents::GatewayIntents` — documented intent bits, `union`, `contains`,
  `bits`; serde as a number.
- `crate::error::{Error, HttpError, GatewayError}` —
  `Error::{Http, Gateway, Json, Io}`; `HttpError::{Unsuccessful{status, code,
  message}, RateLimited{retry_after, global}, Transport(String)}` plus
  `status()`/`code()` accessors; `GatewayError::{Closed{code, reason, fatal},
  Protocol(String), Transport(String)}`. Use these; do not add a second error
  type.

## `model` — wire models

Module layout: `model/mod.rs` declares `application`, `audit_log`, `channel`,
`emoji`, `guild`, `message`, `ready`, `user`. Add submodules only inside
`model/`.

Derive `Debug` (and `Clone` where useful). Deserialise Discord payloads with
`#[serde(default)]` on optional fields, and use `Option<T>` wherever the docs mark
a field nullable or optional. **Unknown fields must be ignored** (serde's
default). Do not fail a whole event because one optional field is missing.

### `model::user`

```rust
pub struct User {
    pub id: UserId,
    pub username: String,
    pub discriminator: String,          // "0" for migrated accounts; default ""
    pub global_name: Option<String>,
    pub avatar: Option<String>,         // hash, not a URL
    pub bot: bool,
    pub system: bool,
    pub flags: Option<u64>,
    pub accent_color: Option<u32>,
    pub banner: Option<String>,
}

impl User {
    /// global_name if set, else username.
    pub fn display_name(&self) -> &str;
    /// Avatar URL; falls back to the default-avatar URL when `avatar` is None.
    pub fn avatar_url(&self) -> String;
    /// `https://cdn.discordapp.com/embed/avatars/{index}.png`.
    pub fn default_avatar_url(&self) -> String;
}
```

Default avatar index is documented in `docs/reference.md` (Image Formatting,
footnote **): migrated/new-username users (discriminator `"0"`) use
`(user_id >> 22) % 6`; legacy users use `discriminator % 5`. Avatar URL is
`https://cdn.discordapp.com/avatars/{user_id}/{hash}.png` (append `?size=` only if
wanted; not required).

### `model::channel`

```rust
pub enum ChannelKind { GuildText, Dm, GuildVoice, GroupDm, GuildCategory,
    GuildAnnouncement, AnnouncementThread, PublicThread, PrivateThread,
    GuildStageVoice, GuildDirectory, GuildForum, GuildMedia, Unknown(i64) }

pub enum OverwriteKind { Role, Member }               // 0 / 1 on the wire
pub struct PermissionOverwrite {
    pub id: GenericId,       // role id or user id depending on `kind`
    pub kind: OverwriteKind,
    pub allow: Permissions,
    pub deny: Permissions,
}
pub struct Channel {
    pub id: ChannelId,
    pub kind: ChannelKind,
    pub guild_id: Option<GuildId>,
    pub parent_id: Option<ChannelId>,
    pub name: Option<String>,
    pub position: Option<i64>,
    pub permission_overwrites: Vec<PermissionOverwrite>, // default empty
    pub topic: Option<String>,
    pub nsfw: Option<bool>,
}
impl ChannelKind { pub fn is_thread(self) -> bool; }   // 10/11/12
```

`Channel` must serialise/deserialise from the channel object in
`docs/channel.md`; `kind` is the integer `type` field (custom serde).

### `model::guild`

```rust
pub struct Role {
    pub id: RoleId, pub name: String, pub color: u32, pub hoist: bool,
    pub position: i64, pub permissions: Permissions, pub managed: bool,
    pub mentionable: bool, pub icon: Option<String>, pub unicode_emoji: Option<String>,
}

pub struct Member {
    pub user: User,
    pub guild_id: Option<GuildId>,     // filled by the gateway from the event envelope
    pub nick: Option<String>,
    pub roles: Vec<RoleId>,
    pub joined_at: Option<Timestamp>,
    pub deaf: bool,
    pub mute: bool,
    pub pending: Option<bool>,
    pub permissions: Option<Permissions>, // interaction members include the computed set
}
impl Member { pub fn display_name(&self) -> &str; }

pub struct Guild {
    pub id: GuildId,
    pub name: String,
    pub icon: Option<String>,
    pub owner_id: UserId,
    pub roles: HashMap<RoleId, Role>,
    pub channels: HashMap<ChannelId, Channel>,
    pub threads: Vec<Channel>,
    pub members: HashMap<UserId, Member>, // from GUILD_CREATE when present
    pub features: Vec<String>,
    pub member_count: Option<i64>,
}
impl Guild {
    pub fn is_owner(&self, user_id: UserId) -> bool;
    /// Permission resolution documented in docs/permissions.md:
    /// owner => all documented permissions; otherwise @everyone role bits,
    /// then channel overwrites for @everyone, roles and the member, with
    /// ADMINISTRATOR bypassing overwrites. Threads use their parent channel's
    /// overwrites (docs/permissions.md "Inherited Permissions (Threads)").
    pub fn permissions_for(&self, member: &Member, channel: &Channel) -> Permissions;
    pub fn member_permissions(&self, member: &Member) -> Permissions; // guild-level only
}
```

`Guild` deserialises from the GUILD_CREATE payload (`docs/gateway-events.md`
Guild Create Extra Fields + guild object in `docs/guild.md`), so `roles` and
`channels` arrive as arrays but must be stored as maps (`deserialize_with`
helpers). `threads` stays a `Vec<Channel>`. `channels`/`threads`/`members` are
absent on a plain guild object; default to empty.

### `model::emoji`

```rust
pub struct Emoji {
    pub id: Option<EmojiId>,
    pub name: Option<String>,
    pub animated: bool,
    pub roles: Vec<RoleId>,
    pub user: Option<User>,
    #[serde(flatten)] pub flags: EmojiFlags,   // three documented booleans
}
/// The three boolean emoji-object fields, flattened so the wire JSON keeps the
/// documented top-level keys `require_colons`, `managed`, `available`
/// (`docs/emoji.md`, Emoji Structure).
pub struct EmojiFlags {
    pub require_colons: bool,
    pub managed: bool,
    pub available: bool,
}
/// Emoji as accepted by message components (`docs/components-reference.md`,
/// Button Structure and Select Option Structure: the partial emoji object
/// `name`, `id`, `animated`), custom -> { "id": "...", "name": "...", "animated": bool },
/// unicode -> { "name": "..." }.
pub enum ReactionType { Unicode(String), Custom { id: EmojiId, name: Option<String>, animated: bool } }
impl From<Emoji> for ReactionType;   // custom when id is Some
impl From<&str> for ReactionType;    // unicode
```

### `model::audit_log`

```rust
pub enum AuditLogAction { GuildUpdate, ChannelCreate, ..., MemberKick,
    MemberPrune, MemberBanAdd, MemberBanRemove, MemberUpdate, MemberRoleUpdate,
    MemberMove, MemberDisconnect, BotAdd, ..., Other(i64) }
pub struct AuditLogEntry {
    pub id: AuditLogEntryId,
    pub action: AuditLogAction,          // deserialised from `action_type`
    pub user_id: Option<UserId>,
    pub target_id: Option<GenericId>,
    pub reason: Option<String>,
    pub changes: Vec<AuditLogChange>,    // default empty
    pub options: Option<serde_json::Value>,
    pub guild_id: Option<GuildId>,       // extra field of the gateway event
}
pub struct AuditLogChange { pub key: String, pub new_value: Option<serde_json::Value>,
    pub old_value: Option<serde_json::Value> }
```

Action-type values are the table in `docs/audit-log.md` (Audit Log Events).
The full table should be covered; `Other(i64)` is the fallback so an unknown
event never breaks a handler. The bot matches on `action` for
`MemberKick`/`MemberBanAdd`/`MemberBanRemove` and ignores everything else.

### `model::message`

```rust
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
    pub author: User,
    pub content: String,              // default "" (absent without MESSAGE_CONTENT)
    pub timestamp: Timestamp,
    pub edited_timestamp: Option<Timestamp>,
    pub member: Option<Member>,
    pub embeds: Vec<Embed>,
    pub components: Vec<Component>,               // typed component objects
    pub attachments: Vec<Attachment>,
    pub interaction: Option<MessageInteraction>,  // who created an interaction message
    pub pinned: bool,
    pub tts: bool,                                // default false, omitted when false
    pub flags: MessageFlags,                      // typed bitfield (docs/message.md, Message Flags)
    pub kind: Option<i64>,
}
// MessageFlags: transparent u64 newtype with one constant per documented flag
// (EPHEMERAL, IS_COMPONENTS_V2, …), contains/insert/remove and bit operators;
// `Message::is_ephemeral()` / `Message::is_components_v2()` are conveniences.
pub struct Embed { title, kind, description, url, timestamp: Option<Timestamp>,
    color: Option<u32>, footer: Option<EmbedFooter>, image: Option<EmbedImage>,
    thumbnail: Option<EmbedImage>, video: Option<EmbedVideo>, provider: Option<EmbedProvider>,
    author: Option<EmbedAuthor>, fields: Vec<EmbedField> }
```

Embed structures follow `docs/message.md` (Embed Object) and must implement both
`Serialize` and `Deserialize` — outbound payloads reuse the same `Embed` type.
Fields are optional per the docs (`docs/message.md`, Embed Structure).

### `model::application` (interactions and commands) — complete

All five documented interaction types are typed. Unknown interaction types stay
`Interaction::Unknown(i64)`; malformed optional fields never fail a payload.
Every interaction type's `Debug` redacts its token.

```rust
pub enum Interaction {
    Ping(PingInteraction),
    Command(CommandInteraction),
    Component(ComponentInteraction),
    Autocomplete(AutocompleteInteraction),   // type 4
    ModalSubmit(ModalSubmitInteraction),     // type 5
    Unknown(i64),
}
impl Interaction { pub fn kind(&self) -> InteractionType; }

pub enum InteractionType { Ping, ApplicationCommand, MessageComponent,
    ApplicationCommandAutocomplete, ModalSubmit, Unknown(i64) }
pub enum InteractionContext { Guild, BotDm, PrivateChannel, Unknown(i64) }
pub enum InteractionCallbackType { Pong, ChannelMessageWithSource,
    DeferredChannelMessageWithSource, DeferredUpdateMessage, UpdateMessage,
    ApplicationCommandAutocompleteResult, Modal, PremiumRequired, LaunchActivity,
    Unknown(i64) }

pub struct PingInteraction { id: InteractionId, application_id: ApplicationId,
    token: String, version: i64 }

pub struct CommandInteraction { id, application_id, token, version: i64,
    guild_id: Option<GuildId>, channel_id: Option<ChannelId>,
    member: Option<Member>, user: User, data: CommandData,
    app_permissions: Option<Permissions>, locale, guild_locale,
    context: Option<InteractionContext>,
    authorizing_integration_owners: Option<AuthorizingIntegrationOwners> }

pub struct AutocompleteInteraction { /* same common fields as CommandInteraction */
    data: CommandData }
impl AutocompleteInteraction { pub fn focused_option(&self) -> Option<&CommandDataOption>; }

pub struct ComponentInteraction { /* common fields */ message: Option<Message>,
    data: ComponentData }
pub struct ModalSubmitInteraction { /* common fields */ message: Option<Message>,
    data: ModalSubmitData }

pub struct CommandData { id: CommandId, name: String, kind: CommandKind,
    resolved: Option<ResolvedData>, options: Vec<CommandDataOption>,
    guild_id: Option<GuildId>, target_id: Option<GenericId> }

pub enum CommandKind { ChatInput, User, Message, PrimaryEntryPoint }
pub enum CommandOptionKind { Subcommand, SubcommandGroup, Str, Integer,
    Boolean, User, Channel, Role, Mentionable, Number, Attachment }
pub enum CommandOptionValue { String(String), Integer(i64), Boolean(bool),
    User(UserId), Channel(ChannelId), Role(RoleId), Mentionable(GenericId),
    Number(f64), Attachment(AttachmentId), Subcommand(Vec<CommandDataOption>),
    SubcommandGroup(Vec<CommandDataOption>) }
pub struct CommandDataOption { name: String, kind: CommandOptionKind,
    value: CommandOptionValue, focused: bool }
// Accessors live on CommandOptionValue: as_str, as_i64, as_bool, as_f64,
// as_user_id, as_channel_id, as_role_id.

pub struct ResolvedData {
    pub users: HashMap<UserId, User>,
    pub members: HashMap<UserId, PartialMember>,
    pub roles: HashMap<RoleId, Role>,
    pub channels: HashMap<ChannelId, Channel>,   // lenient partial channels
    pub messages: HashMap<MessageId, Message>,
    pub attachments: HashMap<AttachmentId, Attachment>,
}
impl ResolvedData { pub fn user/member/role/channel/message/attachment(&self, id)
    -> Option<&...>; }

pub struct ComponentData { custom_id: String, component_kind: i64,
    values: Vec<String>, resolved: Option<ResolvedData> }
pub struct ModalSubmitData { custom_id: String, components: Vec<Component>,
    resolved: Option<ResolvedData> }

pub struct MessageInteraction { id: InteractionId, kind: InteractionType,
    name: String, user: User, member: Option<PartialMember> }
pub struct InteractionCallbackResponse { interaction: InteractionCallback,
    resource: Option<InteractionCallbackResource> }
impl InteractionCallbackResponse { pub fn created_message(&self) -> Option<&Message>; }

/// Registered command, as returned by the registration endpoint.
pub struct Command { id: CommandId, application_id: ApplicationId,
    guild_id: Option<GuildId>, name: String, description: String,
    options: Vec<CommandOption>, default_member_permissions: Option<Permissions> }
pub struct CommandOption { kind: CommandOptionKind, name, description,
    required: Option<bool>, autocomplete: Option<bool>,
    min_value: Option<serde_json::Value>, max_value: Option<serde_json::Value> }
```

Option values are deserialised from Discord's `{ "name", "type", "value" }`
shape (`docs/receiving-and-responding.md`, Application Command Interaction Data
Option Structure: type 3 string, 4 integer, 5 boolean, 6/7/8/9 snowflake
strings, 10 number, 11 attachment snowflake; subcommand/group carry nested
`options` and a `focused` flag for autocomplete). Add unit tests for every
option kind.

### `model::component`

Typed component objects for **every** documented component type: Action Row
(`1`), Button (`2`, with `ButtonStyle`), String Select (`3`, with
`SelectOption`), Text Input (`4`, with `TextInputStyle`), User/Role/
Mentionable/Channel Select (`5`/`6`/`7`/`8`), Section (`9`, with
`SectionAccessory`), Text Display (`10`), Thumbnail (`11`), Media Gallery
(`12`, with `MediaGalleryItem`), File (`13`), Separator (`14`, with
`SeparatorSpacing`), Container (`17`), Label (`18`), File Upload (`19`), Radio
Group (`21`), Checkbox Group (`22`) and Checkbox (`23`). `UnfurledMediaItem`
models the documented media item. `Component::Other(UnknownComponent { kind,
raw })` remains only for type numbers Discord adds later (or a malformed known
body), so an inbound message or modal submit never fails. The interaction
response structures reuse the same types: `TextInput::value`,
`StringSelect::values`, the auto-populated selects' `values`, File Upload
`values`, Radio Group `value` and Checkbox Group `values`/Checkbox `value` are
filled in by modal submit payloads. `Component::Section`/`Container` are
recursed into by `find_component`.

Reference: `docs/components-reference.md` (Component Object, one section per
component type, and each component's Interaction Response Structure).

### `model::ready`

```rust
pub struct Ready {
    pub user: User,
    pub guilds: Vec<UnavailableGuild>,
    pub session_id: String,
    pub resume_gateway_url: String,
    pub shard: Option<(i64, i64)>,
    pub application: PartialApplication,
}
pub struct UnavailableGuild { pub id: GuildId, pub unavailable: Option<bool> }
pub struct PartialApplication { pub id: ApplicationId, pub flags: Option<u64> }
pub struct Resumed;   // dispatched with no fields
```

Source: `docs/gateway-events.md` (Ready, Resumed).

## `builder` — outbound payloads

Builders serialise to the documented JSON shape. Expose a `to_json(&self) ->
serde_json::Value` (or `impl Serialize`) on each top-level builder so the HTTP
layer can send it and tests can assert the shape. Builders do not perform I/O.

### `builder::command`

```rust
pub struct CommandBuilder { /* … */ }
impl CommandBuilder {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self;
    pub fn add_option(self, option: CommandOptionBuilder) -> Self;
    pub fn default_member_permissions(self, permissions: Permissions) -> Self;
    pub fn dm_permission(self, enabled: bool) -> Self;
    pub fn nsfw(self, nsfw: bool) -> Self;
    pub fn to_json(&self) -> serde_json::Value;
}
pub struct CommandOptionBuilder { /* … */ }
impl CommandOptionBuilder {
    pub fn new(kind: CommandOptionKind, name: impl Into<String>,
               description: impl Into<String>) -> Self;
    pub fn required(self, required: bool) -> Self;
    pub fn min_int_value(self, value: i64) -> Self;
    pub fn max_int_value(self, value: i64) -> Self;
    pub fn min_number_value(self, value: f64) -> Self;
    pub fn max_number_value(self, value: f64) -> Self;
    pub fn channel_types(self, types: Vec<ChannelKind>) -> Self;
    pub fn autocomplete(self, autocomplete: bool) -> Self;
    pub fn choice(self, name: impl Into<String>,
                  value: impl Into<CommandOptionChoiceValue>) -> Self;
    pub fn to_json(&self) -> serde_json::Value;
}
pub enum CommandOptionChoiceValue { String(String), Integer(i64) }
```

Choices serialise as the documented Application Command Option Choice Structure
(`name` 1-100 characters, string `value` up to 100 characters, max 25 choices)
and take precedence over `autocomplete`, which the docs forbid combining. The
`/role` command uses fixed `add`/`remove` choices for its `execute` option. The
wire shape is documented in `docs/application-commands.md`; an example is

```json
{"name":"ban","description":"Bans a user from the guild.",
 "default_member_permissions":"4","options":[
   {"type":6,"name":"target","description":"...","required":true},
   {"type":4,"name":"duration","description":"...","min_value":0,"max_value":7}]}
```

`default_member_permissions` is a **decimal permission string**; `type` is the
option-kind integer; `min_value`/`max_value` are numbers. Up to 25 options.

### `builder::embed`

```rust
pub struct EmbedBuilder;
impl EmbedBuilder {
    pub fn new() -> Self;
    pub fn title(self, title: impl Into<String>) -> Self;
    pub fn description(self, description: impl Into<String>) -> Self;
    pub fn url(self, url: impl Into<String>) -> Self;
    pub fn color(self, color: u32) -> Self;
    pub fn timestamp(self, timestamp: Timestamp) -> Self;
    pub fn author(self, author: EmbedAuthorBuilder) -> Self;
    pub fn footer(self, footer: EmbedFooterBuilder) -> Self;
    pub fn field(self, field: EmbedFieldBuilder) -> Self;
    pub fn build(self) -> Embed;
}
pub struct EmbedAuthorBuilder { pub fn new(name: impl Into<String>) -> Self;
    pub fn url(self, url: impl Into<String>) -> Self;
    pub fn icon_url(self, url: impl Into<String>) -> Self; }
pub struct EmbedFooterBuilder { /* new(text), icon_url */ }
pub struct EmbedFieldBuilder { /* new(name, value, inline) */ }
```

Limits from `docs/message.md` (Embed Limits): 256-char title, 4096-char
description, 25 fields, 256-char field name, 1024-char field value, 6000 total
characters.

### `builder::message`

```rust
pub struct MessageBuilder;
impl MessageBuilder {
    pub fn new() -> Self;
    pub fn content(self, content: impl Into<String>) -> Self;   // max 2000 chars
    pub fn embed(self, embed: impl Into<Embed>) -> Self;
    pub fn embeds(self, embeds: impl IntoIterator<Item = Embed>) -> Self; // max 10
    pub fn to_json(&self) -> serde_json::Value;
}
```

Request body: `docs/channel.md` (Create Message JSON Params):
`{"content"?, "embeds"?, "components"?, "allowed_mentions"?}`. The builder sends
no `allowed_mentions` unless the caller sets one, preserving current bot
behaviour.

### `builder::interaction` — complete callback surface

```rust
pub enum InteractionResponse {
    Pong,                                   // {"type":1}
    Reply(ReplyBuilder),                    // {"type":4,"data":{...}}
    Defer(DeferBuilder),                    // {"type":5,"data":{"flags"?:64}}
    DeferUpdate(DeferUpdateBuilder),        // {"type":6}  (no loading state)
    UpdateMessage(UpdateMessageBuilder),    // {"type":7,"data":{...}}
    Autocomplete(AutocompleteBuilder),      // {"type":8,"data":{"choices":[...]}}
    Modal(ModalBuilder),                    // {"type":9,"data":{...}}
    PremiumRequired,                        // {"type":10} deprecated
    LaunchActivity,                         // {"type":12}
}
impl InteractionResponse { pub fn to_json(&self) -> serde_json::Value; }

pub struct ReplyBuilder;          // content, embed(s), button/component(s), tts,
                                  // allowed_mentions, ephemeral (flag 64)
pub struct DeferBuilder;          // ephemeral flag
pub struct DeferUpdateBuilder;
pub struct UpdateMessageBuilder;  // content, embed(s), component(s), tts, allowed_mentions
pub struct AutocompleteBuilder;   // choice(name, value), choices; max 25
pub struct ModalBuilder;          // new(custom_id, title), label(..), action_row(..), component(..)
pub struct InteractionEditBuilder; // PATCH @original: content, embed(s), component(s), allowed_mentions

// Components (docs/components-reference.md).
pub struct ActionRowBuilder;      // button, text_input, string_select, user_select,
                                  // role_select, mentionable_select, channel_select
pub struct ButtonBuilder;         // link/primary/secondary/success/danger/premium, label, emoji, disabled
pub struct StringSelectBuilder; impl StringSelectBuilder { option, options, placeholder, min_values, max_values, required, disabled }
pub struct UserSelectBuilder; pub struct RoleSelectBuilder;
pub struct MentionableSelectBuilder; pub struct ChannelSelectBuilder;
pub struct TextInputBuilder;      // short/paragraph, placeholder, min_length, max_length, required, value
pub struct LabelBuilder;          // label, description, one child interactive component
// `IS_COMPONENTS_V2` layout/content builders (docs/components-reference.md):
pub struct UnfurledMediaItemBuilder; pub struct TextDisplayBuilder;
pub struct ThumbnailBuilder; pub struct MediaGalleryBuilder; pub struct MediaGalleryItemBuilder;
pub struct FileBuilder; pub struct SeparatorBuilder; pub struct SectionBuilder;
pub struct SectionAccessoryBuilder; pub struct ContainerBuilder; pub struct FileUploadBuilder;
pub struct GroupOptionBuilder; pub struct RadioGroupBuilder; pub struct CheckboxGroupBuilder;
pub struct CheckboxBuilder;
pub enum MessageComponentBuilder; // unions all 20 outbound component builders
pub const IS_COMPONENTS_V2_FLAG: u64 = 1 << 15;   // message flag
pub const MAX_MESSAGE_COMPONENTS: usize = 40;
```
`MessageBuilder`, `ReplyBuilder`, `UpdateMessageBuilder` and
`InteractionEditBuilder` accept `impl Into<MessageComponentBuilder>`. When any
attached component is an `IS_COMPONENTS_V2` component the builders set the
documented flag; message/reply/update omit the content fields the API rejects
alongside it, and the edit builder resets `content`/`embeds`/`poll` as the
documented edit rules require (`docs/message.md`, Create Message and Edit
Message footnotes). `LabelBuilder` additionally accepts the modal-only inputs
through `.component(...)`, `.file_upload(...)`, `.radio_group(...)`,
`.checkbox_group(...)` and `.checkbox(...)`.

Rules from the docs and enforced (infallibly, by clipping) by the builders:
5 buttons or exactly one select per action row; at most 5 rows per
message/reply/update; at most 40 components per message; `custom_id` 1-100
chars; button label <=80 and url <=512; select option label/value/description
<=100 with <=25 options; text input value/length <=4000; modal title <=45 with
1-5 components; autocomplete <=25 choices; label text <=45. `ButtonBuilder`
styles map to 1 primary, 2 secondary, 3 success, 4 danger, 5 link (url) and 6
premium (`sku_id`). Callback bodies follow
`docs/receiving-and-responding.md` (Interaction Response Object: Interaction
Callback Type + Interaction Callback Data Structure).

`MessageBuilder` mirrors the same component/tts/flags/ephemeral/allowed-mentions
surface, and `builder::message::AllowedMentionsBuilder` serialises the
documented allowed-mentions object. It also supports `poll(PollBuilder)`
(documented Poll Create Request Object, `docs/poll.md`) and
`attachment(AttachmentBuilder)`/`attachments(..)` file uploads
(`docs/reference.md`, Uploading Files; `docs/message.md`, Attachment Object).
Attachment builders carry the file bytes plus the documented request metadata;
when files are present the HTTP layer sends `multipart/form-data` with a
`payload_json` part and `files[n]` parts (hand-written RFC 7578 encoder, no new
dependencies). The same poll/attachment support exists on `ReplyBuilder`,
`UpdateMessageBuilder` and `InteractionEditBuilder` where the docs allow it,
including `.keep_attachment(id)` for the v10 edit-retention rule.

### `builder::presence`

```rust
pub enum Status { Online, Idle, DoNotDisturb, Invisible, Offline }  // "online", ...
pub enum Activity { Custom{ name: String }, Playing{ name: String },
    Listening{ name }, Watching{ name }, Competing{ name }, Streaming{ name, url } }
impl Activity { pub fn custom(name: impl Into<String>) -> Self; pub fn to_json(&self) -> serde_json::Value;
    // {"name":"Playing with code.","state":"Playing with code.","type":4} for Custom
}
pub struct PresenceBuilder;
impl PresenceBuilder {
    pub fn new() -> Self;
    pub fn status(self, status: Status) -> Self;
    pub fn activity(self, activity: Activity) -> Self;
    pub fn activities(self, activities: impl IntoIterator<Item = Activity>) -> Self;
    pub fn afk(self, afk: bool) -> Self;
    pub fn to_json(&self) -> serde_json::Value; // presence object: {"since":null,"activities":[...],"status":"online","afk":false}
}
```

Source: `docs/gateway-events.md` (Update Presence, Activity Object).

## `http` — REST

```rust
#[derive(Clone)]
pub struct HttpClient { /* token, reqwest::Client, rate limiter, application id */ }
impl HttpClient {
    pub fn new(token: impl Into<String>) -> Self;
    pub fn set_application_id(&self, id: ApplicationId);
    pub fn application_id(&self) -> Option<ApplicationId>;

    pub async fn send_message(&self, channel_id: ChannelId,
        message: &MessageBuilder) -> Result<Message, Error>;
    pub async fn ban_member(&self, guild_id: GuildId, user_id: UserId,
        delete_message_days: u8, reason: Option<&str>) -> Result<(), Error>;
    pub async fn kick_member(&self, guild_id: GuildId, user_id: UserId,
        reason: Option<&str>) -> Result<(), Error>;
    pub async fn set_guild_commands(&self, application_id: ApplicationId,
        guild_id: GuildId, commands: &[CommandBuilder]) -> Result<Vec<Command>, Error>;
    pub async fn set_global_commands(&self, application_id: ApplicationId,
        commands: &[CommandBuilder]) -> Result<Vec<Command>, Error>;
    pub async fn get_application_emoji(&self, application_id: ApplicationId,
        emoji_id: EmojiId) -> Result<Emoji, Error>;
    pub async fn create_interaction_response(&self, interaction_id: InteractionId,
        interaction_token: &str, response: InteractionResponse) -> Result<(), Error>;
    /// Same callback with the documented `?with_response=true`, returning the
    /// created message.
    pub async fn create_interaction_response_with_response(&self, interaction_id: InteractionId,
        interaction_token: &str, response: InteractionResponse)
        -> Result<InteractionCallbackResponse, Error>;
    pub async fn get_interaction_response(&self, application_id: ApplicationId,
        interaction_token: &str) -> Result<Message, Error>;
    pub async fn delete_interaction_response(&self, application_id: ApplicationId,
        interaction_token: &str) -> Result<(), Error>;
    pub async fn edit_interaction_response(&self, application_id: ApplicationId,
        interaction_token: &str, edit: InteractionEditBuilder) -> Result<Message, Error>;
    pub async fn create_followup_message(&self, application_id: ApplicationId,
        interaction_token: &str, message: &MessageBuilder) -> Result<Message, Error>;
    pub async fn get_followup_message(&self, application_id: ApplicationId,
        interaction_token: &str, message_id: MessageId) -> Result<Message, Error>;
    pub async fn edit_followup_message(&self, application_id: ApplicationId,
        interaction_token: &str, message_id: MessageId, edit: &InteractionEditBuilder)
        -> Result<Message, Error>;
    pub async fn delete_followup_message(&self, application_id: ApplicationId,
        interaction_token: &str, message_id: MessageId) -> Result<(), Error>;
}
```

All interaction endpoints are unauthenticated (the interaction token is the
credential) and exempt from the bot's global rate-limit bucket; interaction
tokens are secrets, so nothing logs them.

`http/impls.rs` adds the conveniences the bot and users call:

```rust
impl ChannelId { pub async fn send_message(&self, http: &HttpClient, message: &MessageBuilder) -> Result<Message, Error>; }
impl GuildId { pub async fn set_commands(&self, http: &HttpClient, commands: &[CommandBuilder]) -> Result<Vec<Command>, Error>; }
impl ApplicationId { pub async fn set_global_commands(&self, http: &HttpClient, commands: &[CommandBuilder]) -> Result<Vec<Command>, Error>; }
impl PingInteraction { pub async fn ack(&self, http: &HttpClient) -> Result<(), Error>; }
impl CommandInteraction {
    pub async fn respond(&self, http, response: impl Into<InteractionResponse>) -> Result<(), Error>;
    pub async fn respond_with_response(&self, http, response) -> Result<InteractionCallbackResponse, Error>;
    pub async fn respond_modal(&self, http, modal: ModalBuilder) -> Result<(), Error>;
    pub async fn defer(&self, http) -> Result<(), Error>;
    pub async fn defer_ephemeral(&self, http) -> Result<(), Error>;
    pub async fn edit_response(&self, http, edit: InteractionEditBuilder) -> Result<Message, Error>;
    pub async fn get_response(&self, http) -> Result<Message, Error>;
    pub async fn delete_response(&self, http) -> Result<(), Error>;
    pub async fn followup(&self, http, message: &MessageBuilder) -> Result<Message, Error>;
    pub async fn get_followup/edit_followup/delete_followup(...);
}
impl ComponentInteraction {
    // respond, defer_update, update_message, respond_modal, edit_response,
    // get_response, delete_response, followup CRUD
}
impl ModalSubmitInteraction {
    // respond, defer_update, update_message, edit_response, get/delete response, followup CRUD
}
impl AutocompleteInteraction {
    pub async fn respond(&self, http, response) -> Result<(), Error>;
    pub async fn respond_choices(&self, http, choices: impl IntoIterator<Item = AutocompleteChoice>) -> Result<(), Error>;
}
```
```

Required protocol behaviour:

- Base URL `https://discord.com/api/v10`; bot requests send
  `Authorization: Bot <token>`; every request sends
  `User-Agent: DiscordBot (https://github.com/gage-lodba/discord-bot-template, <crate version>)`.
  (`docs/reference.md` Authentication, User Agent.)
- Interaction endpoints are unauthenticated (they carry the interaction token in
  the path) and are **not** subject to the bot global limit.
  (`docs/rate-limits.md`, Global Rate Limit.)
- Audit-log reasons go into `X-Audit-Log-Reason` percent-encoded (UTF-8, 1-512
  characters). (`docs/audit-log.md`.)
- Rate limits (`docs/rate-limits.md`): track `X-RateLimit-Bucket` +
  `X-RateLimit-Remaining` + `X-RateLimit-Reset-After`; a request on an exhausted
  bucket waits until reset; on `429` wait `Retry-After` (or `retry_after` in the
  body) and retry, honouring `global`/`X-RateLimit-Global`; 5xx responses retry
  with a small bounded backoff. Cap retries (e.g. 5) and return
  `HttpError::RateLimited`/`Unsuccessful` when exhausted.
- Non-success responses parse `{"code": i64, "message": string}` into
  `HttpError::Unsuccessful`; a body that is not JSON still yields
  `Unsuccessful` with the status and the raw text as message.
- Empty (204) responses are successes with no value.
- `ban_member` uses `delete_message_seconds` (0-604800 = days * 86400), the
  endpoint's current field, not the deprecated `delete_message_days`
  (`docs/guild.md` Create Guild Ban).
- Routes used (all under `/v10`):
  - `POST /channels/{channel_id}/messages`
  - `PUT /guilds/{guild_id}/bans/{user_id}`
  - `DELETE /guilds/{guild_id}/members/{user_id}`
  - `PUT /applications/{application_id}/guilds/{guild_id}/commands`
  - `PUT /applications/{application_id}/commands`
  - `GET /applications/{application_id}/emojis/{emoji_id}`
  - `POST /interactions/{interaction_id}/{interaction_token}/callback`
  - `PATCH /webhooks/{application_id}/{interaction_token}/messages/@original`

## `gateway`, `cache`, `client`

```rust
pub struct Client<H: EventHandler> {
    pub data: Arc<tokio::sync::RwLock<Data>>,
    pub cache: Arc<Cache>,
    pub http: HttpClient,
    pub shard_manager: Arc<ShardManager>,
    /* handler, token, intents, ... private */
}
impl<H: EventHandler> Client<H> { pub async fn start(&mut self) -> Result<(), Error>; }

impl Client<()> {
    pub fn builder(token: impl Into<String>, intents: GatewayIntents) -> ClientBuilder<()>;
}
pub struct ClientBuilder<H> { /* ... */ }
impl ClientBuilder<()> { pub fn event_handler<H2: EventHandler>(self, handler: H2) -> ClientBuilder<H2>; }
impl<H: EventHandler> std::future::IntoFuture for ClientBuilder<H> {
    type Output = Result<Client<H>, Error>;   // so `.await` works
}

pub trait DataKey: 'static { type Value: Send + Sync + 'static; }
pub struct Data;                 // type map
impl Data { pub fn insert<K: DataKey>(&mut self, value: K::Value);
            pub fn get<K: DataKey>(&self) -> Option<&K::Value>; }

#[derive(Clone)]
pub struct Context {
    pub http: HttpClient,
    pub cache: Arc<Cache>,
    pub data: Arc<tokio::sync::RwLock<Data>>,
    /* shard handle private */
}
impl Context {
    pub async fn get_application_emoji(&self, emoji_id: EmojiId) -> Result<Emoji, Error>;
    pub fn update_presence(&self, presence: &PresenceBuilder) -> Result<(), Error>;
}

// The methods return an explicit `impl Future + Send` rather than `async fn`:
// dispatch spawns each handler future, which requires a provable `Send` bound
// that async-fn-in-trait does not provide. Call sites are identical, and
// implementing the trait with `async fn` still compiles.
pub trait EventHandler: Send + Sync + 'static {
    fn on_ready(&self, _ctx: Context, _ready: Ready) -> impl Future<Output = ()> + Send { async {} }
    fn on_resumed(&self, _ctx: Context, _event: Resumed) -> impl Future<Output = ()> + Send { async {} }
    fn on_message_create(&self, _ctx: Context, _message: Message) -> impl Future<Output = ()> + Send { async {} }
    fn on_interaction_create(&self, _ctx: Context, _interaction: Interaction) -> impl Future<Output = ()> + Send { async {} }
    fn on_guild_member_add(&self, _ctx: Context, _member: Member) -> impl Future<Output = ()> + Send { async {} }
    fn on_guild_member_remove(&self, _ctx: Context, _guild_id: GuildId,
        _user: User, _member: Option<Member>) -> impl Future<Output = ()> + Send { async {} }
    fn on_guild_ban_add(&self, _ctx: Context, _guild_id: GuildId, _user: User) -> impl Future<Output = ()> + Send { async {} }
    fn on_guild_ban_remove(&self, _ctx: Context, _guild_id: GuildId, _user: User) -> impl Future<Output = ()> + Send { async {} }
    fn on_guild_audit_log_entry_create(&self, _ctx: Context,
        _entry: AuditLogEntry, _guild_id: GuildId) -> impl Future<Output = ()> + Send { async {} }
}
impl EventHandler for () {}

pub struct Cache;
impl Cache {
    pub fn current_user(&self) -> Option<Arc<User>>;
    pub fn guild(&self, guild_id: GuildId) -> Option<Arc<Guild>>;
    pub fn guilds(&self) -> Vec<Arc<Guild>>;
    pub fn application_id(&self) -> Option<ApplicationId>;
}

pub struct ShardManager { /* ... */ }
impl ShardManager { pub async fn shutdown_all(&self); }
```

Behaviour required by the bot (and testable):

- Connect to `wss://gateway.discord.gg/?v=10&encoding=json`.
- On Hello (op 10): wait `heartbeat_interval * jitter` (jitter random 0..=1),
  then send Heartbeat (op 1) every interval with the last sequence number, or
  `null` if none. Track ACKs (op 11); if a heartbeat goes unacknowledged, close
  with a non-1000/1001 code and reconnect+resume. Listen for server-requested
  heartbeats (op 1) and answer immediately.
- Identify (op 2) with token, intents, `properties: {os, browser, device}` and
  `compress: false`. Resume (op 6) with token, session_id and last seq.
- Dispatch (op 0) updates the cache and then invokes the matching `on_*`
  handler. Handlers run in spawned tasks so a slow handler (member-removal waits
  1.5 s) cannot stall heartbeats.
- INTERACTION_CREATE with a PING interaction (type 1) is acknowledged
  automatically with the documented PONG callback (`{"type":1}`) inside the
  3 second deadline, independently of handler delivery; failures are logged as
  warnings, and the interaction is still delivered to
  `on_interaction_create`. All other interaction types go straight to the
  handler.
- Reconnect (op 7), close codes and Invalid Session (op 9) per
  `docs/opcodes-and-status-codes.md`: codes 4004/4010/4011/4012/4013/4014 are
  fatal (return `GatewayError::Closed { fatal: true }` from `start`); other
  closes reconnect using `resume_gateway_url` when a session exists, otherwise
  re-identify after an exponential backoff (start ~1s, cap ~60s). Op 9 with
  `d: true` resumes; `d: false` re-identifies after 1-5s.
- READY: cache the current user, guild list and application id; set the HTTP
  client's application id; call `on_ready`.
- GUILD_CREATE: cache the guild (available only). GUILD_MEMBER_ADD: fill
  `member.guild_id` from the event's `guild_id` and cache the guild's member.
- Shutdown: `shutdown_all()` asks the shard to close with code 1000/1001 and
  waits for it; `start()` then returns `Ok(())`. `main.rs` waits up to 10 s.
- `Context::update_presence` sends an op 3 payload over the live connection.
- Do not log message content at INFO (the bot deliberately uses DEBUG).

## Bot usage in this repository

`src/**` is the single-guild moderation bot written against exactly this
contract:

```rust
use discord::{Client, GatewayIntents};
Client::builder(&config.token, INTENTS).event_handler(events::Handler::new(ids)).await?;
client.data.write().await.insert::<config::Ids>(ids);
client.shard_manager.clone().shutdown_all().await;
client.start().await?;
ctx.cache.current_user(); ctx.cache.guild(guild_id);
guild_id.set_commands(&ctx.http, &builders).await?;
interaction.respond(&ctx.http, ReplyBuilder::new().content("Pong!").ephemeral()).await?;
ctx.http.ban_member(...); ctx.http.kick_member(...);
ctx.update_presence(&PresenceBuilder::new().status(Status::Online).activity(Activity::custom("Playing with code.")))?;
```

The bot registers its slash commands in the configured guild on `READY`
(`GuildId::set_commands`), routes every interaction type in
`src/events/interaction_create.rs` — commands, components, autocomplete and modal
submits — and answers unhandled component/modal ids with an ephemeral message so
no interaction is ever left unacknowledged. Moderation writes re-check the
documented permission hierarchy against the guild cache and fail closed, and each
one carries an audit-log reason. The deployment ids (`DISCORD_GUILD_ID`,
`DISCORD_LOG_CHANNEL_ID`, optional owner/emoji ids) come from the environment;
the crate's `ApplicationId::set_global_commands` is available for a global
command setup but the bot deliberately registers guild-scoped commands.

## Definition of done

- `cargo fmt --all -- --check`,
  `cargo clippy --workspace --release --all-targets --locked -- -D warnings`,
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked`,
  `cargo test --workspace --locked` and `cargo build --release --locked` all
  pass; these are the checks `.github/workflows/Lint_Build_Test.yml` runs.
  The lint set itself is declared once in `[workspace.lints]` in the workspace
  `Cargo.toml` (clippy `all` + `pedantic`, missing docs/Debug, `unsafe_code =
  "forbid"`, unreachable `pub`, rustdoc link lints), so a local `cargo clippy`
  reports exactly what CI denies. No lint is allowed or expected: a warning is
  fixed in the code (checked conversions, helper functions, faithful wire
  modelling) rather than suppressed with `#[allow]`/`#[expect]`.
- Every module has unit tests for its wire parsing/serialisation and its
  non-trivial logic (permission resolution, rate-limit decisions, close-code
  classification, timestamp/ID conversion, builder JSON).
- `crates/discord/tests/public_api.rs` exercises the crate as an external user
  (public API only) against the documented wire payloads.
- No third-party Discord library is wrapped or copied anywhere in the crate.
