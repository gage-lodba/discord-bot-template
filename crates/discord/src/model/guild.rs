//! Guild, role and guild-member models, including the documented permission
//! hierarchy.
//!
//! Reference: `docs/guild.md` (Guild Object, Guild Member Object),
//! `docs/permissions.md` (Role Object, Permission Overwrites) and
//! `docs/gateway-events.md` (Guild Create Extra Fields).

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::id::{ChannelId, GenericId, GuildId, RoleId, UserId};
use crate::model::channel::{Channel, OverwriteKind};
use crate::model::user::User;
use crate::permissions::Permissions;
use crate::timestamp::Timestamp;

/// A guild role: a named set of permissions with a position in the role
/// hierarchy.
///
/// Reference: `docs/permissions.md` (Role Object).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// The role id. The `@everyone` role has the same id as its guild.
    pub id: RoleId,
    /// The role name.
    pub name: String,
    /// Deprecated integer representation of the role colour.
    ///
    /// Discord still returns this field even though the newer `colors` object
    /// is preferred.
    #[serde(default)]
    pub color: u32,
    /// Whether the role is pinned in the member listing.
    #[serde(default)]
    pub hoist: bool,
    /// The position of the role; roles with the same position sort by id.
    #[serde(default)]
    pub position: i64,
    /// The permission bit set granted by this role.
    pub permissions: Permissions,
    /// Whether the role is managed by an integration.
    #[serde(default)]
    pub managed: bool,
    /// Whether the role is mentionable.
    #[serde(default)]
    pub mentionable: bool,
    /// The role's icon hash, if it has one.
    #[serde(default)]
    pub icon: Option<String>,
    /// The role's unicode emoji, if it has one.
    #[serde(default)]
    pub unicode_emoji: Option<String>,
}

/// A guild member: a user plus their guild-specific state.
///
/// The `guild_id` is not part of the member object on the wire; the gateway
/// fills it from the event envelope when it hands the member to a handler.
///
/// Reference: `docs/guild.md` (Guild Member Object).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The user this guild member represents.
    pub user: User,
    /// The guild this member belongs to, filled by the gateway from the event
    /// envelope. `None` when the member arrived in an object without one.
    #[serde(default)]
    pub guild_id: Option<GuildId>,
    /// The user's guild nickname.
    #[serde(default)]
    pub nick: Option<String>,
    /// The ids of the roles assigned to this member.
    #[serde(default)]
    pub roles: Vec<RoleId>,
    /// When the user joined the guild. `None` for members invited as a guest
    /// in `VOICE_STATE_UPDATE` events.
    #[serde(default)]
    pub joined_at: Option<Timestamp>,
    /// Whether the user is deafened in voice channels.
    #[serde(default)]
    pub deaf: bool,
    /// Whether the user is muted in voice channels.
    #[serde(default)]
    pub mute: bool,
    /// Whether the user has not yet passed Membership Screening.
    #[serde(default)]
    pub pending: Option<bool>,
    /// The member's total permissions in the channel including overwrites,
    /// only included on members inside interaction objects.
    #[serde(default)]
    pub permissions: Option<Permissions>,
}

impl Member {
    /// Returns the guild nickname when set, otherwise the user's display name.
    ///
    /// Reference: `docs/guild.md` (Guild Member Object).
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.nick
            .as_deref()
            .unwrap_or_else(|| self.user.display_name())
    }
}

/// A guild, as received in a `GUILD_CREATE` event.
///
/// `roles`, `channels` and `members` arrive as arrays on the wire and are stored
/// as maps; `channels`, `threads` and `members` are absent on a plain guild
/// object and default to empty.
///
/// Reference: `docs/guild.md` (Guild Object) and `docs/gateway-events.md`
/// (Guild Create Extra Fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guild {
    /// The guild id.
    pub id: GuildId,
    /// The guild name.
    pub name: String,
    /// The guild icon hash, if it has one.
    #[serde(default)]
    pub icon: Option<String>,
    /// The id of the guild owner.
    pub owner_id: UserId,
    /// The guild's roles keyed by id, including the `@everyone` role whose id is
    /// the guild id.
    #[serde(
        default,
        deserialize_with = "deserialize_roles",
        serialize_with = "serialize_map_values"
    )]
    pub roles: HashMap<RoleId, Role>,
    /// The guild's channels keyed by id (`GUILD_CREATE` only).
    #[serde(
        default,
        deserialize_with = "deserialize_channels",
        serialize_with = "serialize_map_values"
    )]
    pub channels: HashMap<ChannelId, Channel>,
    /// The guild's active threads (`GUILD_CREATE` only).
    #[serde(default)]
    pub threads: Vec<Channel>,
    /// The guild's members keyed by user id, when the event included them.
    #[serde(
        default,
        deserialize_with = "deserialize_members",
        serialize_with = "serialize_map_values"
    )]
    pub members: HashMap<UserId, Member>,
    /// The enabled guild feature strings (`docs/guild.md`, Guild Features).
    #[serde(default)]
    pub features: Vec<String>,
    /// The total number of members in the guild (`GUILD_CREATE` only).
    #[serde(default)]
    pub member_count: Option<i64>,
}

impl Guild {
    /// Returns whether `user_id` owns this guild.
    ///
    /// Reference: `docs/guild.md` (Guild Object, `owner_id`).
    #[must_use]
    pub fn is_owner(&self, user_id: UserId) -> bool {
        self.owner_id == user_id
    }

    /// Returns the guild-level permissions of a member, without channel
    /// overwrites.
    ///
    /// This follows `docs/permissions.md` (Permission Overwrites,
    /// `compute_base_permissions`): the guild owner gets every documented
    /// permission; otherwise the `@everyone` role's permissions are combined
    /// with the member's other roles, and `ADMINISTRATOR` grants every
    /// documented permission.
    #[must_use]
    pub fn member_permissions(&self, member: &Member) -> Permissions {
        if self.is_owner(member.user.id) {
            return all_permissions();
        }

        let mut permissions = self
            .roles
            .get(&RoleId::new(self.id.get()))
            .map_or_else(Permissions::empty, |role| role.permissions);

        for role_id in &member.roles {
            if let Some(role) = self.roles.get(role_id) {
                permissions.insert(role.permissions);
            }
        }

        if permissions.is_administrator() {
            return all_permissions();
        }

        permissions
    }

    /// Computes a member's permissions in a channel.
    ///
    /// This implements the hierarchy from `docs/permissions.md` (Permission
    /// Overwrites):
    ///
    /// 1. the owner, and members whose base permissions include
    ///    `ADMINISTRATOR`, get every documented permission (administrator
    ///    bypasses channel overwrites);
    /// 2. the `@everyone` channel overwrite is applied (deny, then allow);
    /// 3. the overwrites of the member's roles are combined and applied (all
    ///    denies, then all allows);
    /// 4. the member-specific overwrite is applied (deny, then allow).
    ///
    /// Threads inherit the permissions of their parent channel: the overwrites
    /// of the thread's parent are used instead of the thread's own (which
    /// typically do not exist), per `docs/permissions.md` "Inherited
    /// Permissions (Threads)".
    #[must_use]
    pub fn permissions_for(&self, member: &Member, channel: &Channel) -> Permissions {
        let base = self.member_permissions(member);

        // ADMINISTRATOR overrides any potential permission overwrites.
        if base.is_administrator() {
            return all_permissions();
        }

        let overwrite_source = self.overwrite_source(channel);
        let mut permissions = base;

        // 2. the @everyone overwrite is applied first.
        let everyone_id = GenericId::new(self.id.get());
        if let Some(overwrite) = overwrite_source
            .permission_overwrites
            .iter()
            .find(|overwrite| overwrite.kind == OverwriteKind::Role && overwrite.id == everyone_id)
        {
            permissions.remove(overwrite.deny);
            permissions.insert(overwrite.allow);
        }

        // 3. role-specific overwrites: collect every applicable allow/deny and
        // apply them once, so an allow from one role beats a deny from another
        // regardless of role position (docs/permissions.md, Permission
        // Hierarchy).
        let mut allow = Permissions::empty();
        let mut deny = Permissions::empty();
        for overwrite in &overwrite_source.permission_overwrites {
            if overwrite.kind != OverwriteKind::Role || overwrite.id == everyone_id {
                continue;
            }

            if member
                .roles
                .iter()
                .any(|role_id| role_id.get() == overwrite.id.get())
            {
                allow.insert(overwrite.allow);
                deny.insert(overwrite.deny);
            }
        }
        permissions.remove(deny);
        permissions.insert(allow);

        // 4. member-specific overwrite is applied last.
        let member_id = GenericId::new(member.user.id.get());
        if let Some(overwrite) = overwrite_source
            .permission_overwrites
            .iter()
            .find(|overwrite| overwrite.kind == OverwriteKind::Member && overwrite.id == member_id)
        {
            permissions.remove(overwrite.deny);
            permissions.insert(overwrite.allow);
        }

        permissions
    }

    /// Returns the channel whose permission overwrites apply to `channel`.
    ///
    /// Threads use their parent channel's overwrites; when the parent is not
    /// cached the thread's own overwrites are used instead of failing.
    fn overwrite_source<'a>(&'a self, channel: &'a Channel) -> &'a Channel {
        if !channel.kind.is_thread() {
            return channel;
        }

        channel
            .parent_id
            .and_then(|parent_id| self.channels.get(&parent_id))
            .unwrap_or(channel)
    }
}

/// Returns every permission bit this crate knows about.
///
/// `docs/permissions.md` says `ADMINISTRATOR` "allows all permissions"; the
/// returned set is the documented bit range, not the raw `u64::MAX`.
fn all_permissions() -> Permissions {
    Permissions::from_bits_truncate(u64::MAX)
}

/// Deserialises the guild object's `roles` array into a map keyed by role id.
fn deserialize_roles<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<RoleId, Role>, D::Error> {
    let roles = Vec::<Role>::deserialize(deserializer)?;

    Ok(roles.into_iter().map(|role| (role.id, role)).collect())
}

/// Deserialises the guild object's `channels` array into a map keyed by channel
/// id.
fn deserialize_channels<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<ChannelId, Channel>, D::Error> {
    let channels = Vec::<Channel>::deserialize(deserializer)?;

    Ok(channels
        .into_iter()
        .map(|channel| (channel.id, channel))
        .collect())
}

/// Deserialises the guild object's `members` array into a map keyed by user id.
///
/// Members are parsed leniently: a member object this crate cannot understand
/// is skipped instead of failing the whole `GUILD_CREATE`.
fn deserialize_members<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<UserId, Member>, D::Error> {
    let members = Vec::<serde_json::Value>::deserialize(deserializer)?;

    Ok(members
        .into_iter()
        .filter_map(|value| serde_json::from_value::<Member>(value).ok())
        .map(|member| (member.user.id, member))
        .collect())
}

/// Serialises a map back into the array-of-values shape Discord uses.
fn serialize_map_values<S, Key, Value>(
    map: &HashMap<Key, Value>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    Key: std::hash::Hash + Eq,
    Value: Serialize,
{
    use serde::ser::SerializeSeq;

    let mut sequence = serializer.serialize_seq(Some(map.len()))?;
    for value in map.values() {
        sequence.serialize_element(value)?;
    }
    sequence.end()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a hand-built guild payload resembling the documented
    /// `GUILD_CREATE` shape (`docs/guild.md` + Guild Create Extra Fields).
    fn guild(value: serde_json::Value) -> Guild {
        serde_json::from_value(value).expect("guild fixture should parse")
    }

    /// A guild with two roles, a text channel with no overwrites and one
    /// member (user id 200) who has role 10.
    fn simple_guild() -> Guild {
        guild(serde_json::json!({
            "id": "1",
            "name": "Test Guild",
            "icon": null,
            "owner_id": "100",
            "roles": [
                {
                    "id": "1",
                    "name": "@everyone",
                    "color": 0,
                    "hoist": false,
                    "position": 0,
                    "permissions": "1024",
                    "managed": false,
                    "mentionable": false
                },
                {
                    "id": "10",
                    "name": "Member",
                    "color": 3_447_003,
                    "hoist": true,
                    "position": 1,
                    "permissions": "2048",
                    "managed": false,
                    "mentionable": true
                }
            ],
            "channels": [
                {"id": "20", "type": 0, "guild_id": "1", "name": "general"}
            ],
            "members": [
                {
                    "user": {
                        "id": "200",
                        "username": "member",
                        "discriminator": "0",
                        "global_name": "Member Name"
                    },
                    "nick": null,
                    "roles": ["10"],
                    "joined_at": "2020-01-01T00:00:00.000000+00:00",
                    "deaf": false,
                    "mute": false,
                    "pending": false
                }
            ],
            "features": ["COMMUNITY"],
            "member_count": 2,
            "large": false,
            "voice_states": [],
            "presences": []
        }))
    }

    fn member(roles: &[u64]) -> Member {
        serde_json::from_value(serde_json::json!({
            "user": {"id": "200", "username": "member", "discriminator": "0"},
            "roles": roles.iter().map(u64::to_string).collect::<Vec<_>>(),
            "joined_at": "2020-01-01T00:00:00.000000+00:00",
            "deaf": false,
            "mute": false
        }))
        .expect("member fixture")
    }

    fn channel_with_overwrites(overwrites: &serde_json::Value) -> Channel {
        serde_json::from_value(serde_json::json!({
            "id": "20",
            "type": 0,
            "guild_id": "1",
            "name": "general",
            "permission_overwrites": overwrites
        }))
        .expect("channel fixture")
    }

    fn overwrite(id: u64, kind: i64, allow: &str, deny: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id.to_string(),
            "type": kind,
            "allow": allow,
            "deny": deny
        })
    }

    #[test]
    fn parses_the_documented_role_object() {
        let role: Role = serde_json::from_value(serde_json::json!({
            "id": "41771983423143936",
            "name": "WE DEM BOYZZ!!!!!!",
            "color": 3_447_003,
            "colors": {
                "primary_color": 3_447_003,
                "secondary_color": null,
                "tertiary_color": null
            },
            "hoist": true,
            "icon": "cf3ced8600b777c9486c6d8d84fb4327",
            "unicode_emoji": null,
            "position": 1,
            "permissions": "66321471",
            "managed": false,
            "mentionable": false,
            "flags": 0
        }))
        .expect("documented role payload");

        assert_eq!(role.name, "WE DEM BOYZZ!!!!!!");
        assert_eq!(role.color, 3_447_003);
        assert!(role.hoist);
        assert_eq!(role.position, 1);
        assert!(role.permissions.contains(Permissions::BAN_MEMBERS));
        assert_eq!(
            role.icon.as_deref(),
            Some("cf3ced8600b777c9486c6d8d84fb4327")
        );
        assert_eq!(role.unicode_emoji, None);
    }

    #[test]
    fn parses_a_guild_member_object() {
        let member: Member = serde_json::from_value(serde_json::json!({
            "user": {
                "id": "80351110224678912",
                "username": "Nelly",
                "discriminator": "1337"
            },
            "nick": "NOT API SUPPORT",
            "avatar": null,
            "banner": null,
            "roles": ["41771983423143936"],
            "joined_at": "2015-04-26T06:26:56.936000+00:00",
            "deaf": false,
            "mute": false
        }))
        .expect("documented member payload");

        assert_eq!(member.display_name(), "NOT API SUPPORT");
        assert_eq!(member.roles, vec![RoleId::new(41_771_983_423_143_936)]);
        assert_eq!(
            member.joined_at,
            Some(Timestamp::from_unix_millis(1_430_029_616_936))
        );
        assert_eq!(member.pending, None);
        assert_eq!(member.permissions, None);
        assert_eq!(member.guild_id, None);
    }

    #[test]
    fn member_display_name_falls_back_to_the_user() {
        let member: Member = serde_json::from_value(serde_json::json!({
            "user": {
                "id": "200",
                "username": "username",
                "global_name": "Global Name",
                "discriminator": "0"
            },
            "nick": null,
            "roles": [],
            "joined_at": null,
            "deaf": false,
            "mute": false
        }))
        .expect("member payload");

        assert_eq!(member.display_name(), "Global Name");
    }

    #[test]
    fn deserialises_guild_create_arrays_into_maps() {
        let guild = simple_guild();

        assert_eq!(guild.roles.len(), 2);
        assert_eq!(
            guild
                .roles
                .get(&RoleId::new(10))
                .map(|role| role.name.as_str()),
            Some("Member")
        );
        assert_eq!(guild.channels.len(), 1);
        assert!(guild.channels.contains_key(&ChannelId::new(20)));
        assert_eq!(guild.members.len(), 1);

        let member = guild.members.get(&UserId::new(200)).expect("member");
        assert_eq!(member.display_name(), "Member Name");
        assert_eq!(guild.features, vec!["COMMUNITY".to_owned()]);
        assert_eq!(guild.member_count, Some(2));
        assert_eq!(guild.threads.len(), 0);
    }

    #[test]
    fn plain_guild_object_defaults_the_gateway_only_collections() {
        let guild = guild(serde_json::json!({
            "id": "197038439483310086",
            "name": "Discord Testers",
            "icon": "f64c482b807da4f539cff778d174971c",
            "owner_id": "73193882359173120",
            "features": ["VERIFIED", "COMMUNITY"],
            "roles": [],
            "emojis": [],
            "afk_timeout": 300,
            "verification_level": 3
        }));

        assert!(guild.channels.is_empty());
        assert!(guild.threads.is_empty());
        assert!(guild.members.is_empty());
        assert!(guild.roles.is_empty());
        assert_eq!(guild.member_count, None);
        assert_eq!(
            guild.icon.as_deref(),
            Some("f64c482b807da4f539cff778d174971c")
        );
    }

    #[test]
    fn is_owner_compares_the_owner_id() {
        let guild = simple_guild();

        assert!(guild.is_owner(UserId::new(100)));
        assert!(!guild.is_owner(UserId::new(200)));
    }

    #[test]
    fn owner_gets_every_documented_permission_and_bypasses_overwrites() {
        let guild = simple_guild();
        let owner = member(&[]);
        let mut owner = owner;
        owner.user.id = UserId::new(100);

        let channel = channel_with_overwrites(&serde_json::json!([overwrite(1, 0, "0", "2048")]));

        let permissions = guild.permissions_for(&owner, &channel);

        assert_eq!(permissions, all_permissions());
        assert!(permissions.contains(Permissions::SEND_MESSAGES));
        assert!(permissions.contains(Permissions::ADMINISTRATOR));
        assert_eq!(guild.member_permissions(&owner), all_permissions());
    }

    #[test]
    fn base_permissions_combine_everyone_and_member_roles() {
        let guild = simple_guild();
        let member = member(&[10]);
        let channel = channel_with_overwrites(&serde_json::json!([]));

        let permissions = guild.permissions_for(&member, &channel);

        assert_eq!(
            permissions,
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES
        );
    }

    #[test]
    fn administrator_role_grants_all_permissions_and_bypasses_overwrites() {
        let guild = guild(serde_json::json!({
            "id": "1",
            "name": "Test Guild",
            "owner_id": "100",
            "roles": [
                {"id": "1", "name": "@everyone", "permissions": "1024"},
                {"id": "10", "name": "Admin", "permissions": "8"}
            ],
            "channels": [{"id": "20", "type": 0, "name": "general"}],
            "members": []
        }));
        let member = member(&[10]);
        let channel = channel_with_overwrites(&serde_json::json!([overwrite(200, 1, "0", "2048")]));

        let permissions = guild.permissions_for(&member, &channel);

        assert!(permissions.is_administrator());
        assert_eq!(permissions, all_permissions());
        assert!(permissions.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn everyone_overwrite_deny_then_role_overwrite_allow() {
        // docs/permissions.md Permission Overwrites: @everyone denies, then
        // role allows re-allow.
        let guild = simple_guild();
        let member = member(&[10]);
        let channel = channel_with_overwrites(&serde_json::json!([
            overwrite(1, 0, "0", "2048"),
            overwrite(10, 0, "2048", "0")
        ]));

        let permissions = guild.permissions_for(&member, &channel);

        assert!(permissions.contains(Permissions::VIEW_CHANNEL));
        assert!(permissions.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn a_allow_from_one_role_beats_a_deny_from_another_role() {
        // docs/permissions.md Permission Hierarchy: "A denies the VIEW_CHANNEL
        // permission ... B allows ... The user would ultimately be able to view
        // the channel, regardless of the role positions."
        let guild = guild(serde_json::json!({
            "id": "1",
            "name": "Test Guild",
            "owner_id": "100",
            "roles": [
                {"id": "1", "name": "@everyone", "permissions": "1024"},
                {"id": "10", "name": "A", "permissions": "0"},
                {"id": "11", "name": "B", "permissions": "0"}
            ],
            "channels": [{"id": "20", "type": 0, "name": "general"}],
            "members": []
        }));
        let member = member(&[10, 11]);
        let channel = channel_with_overwrites(&serde_json::json!([
            overwrite(10, 0, "0", "1024"),
            overwrite(11, 0, "1024", "0")
        ]));

        let permissions = guild.permissions_for(&member, &channel);

        assert!(permissions.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn member_overwrite_is_applied_after_role_overwrites() {
        let guild = simple_guild();
        let member = member(&[10]);
        let channel = channel_with_overwrites(&serde_json::json!([
            overwrite(10, 0, "2048", "0"),
            overwrite(200, 1, "0", "2048")
        ]));

        let permissions = guild.permissions_for(&member, &channel);

        assert_eq!(permissions, Permissions::VIEW_CHANNEL);
        assert!(!permissions.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn threads_use_their_parent_channels_overwrites() {
        // docs/permissions.md Inherited Permissions (Threads).
        let guild = guild(serde_json::json!({
            "id": "1",
            "name": "Test Guild",
            "owner_id": "100",
            "roles": [
                {"id": "1", "name": "@everyone", "permissions": "1024"},
                {"id": "10", "name": "Member", "permissions": "2048"}
            ],
            "channels": [
                {
                    "id": "20",
                    "type": 0,
                    "name": "general",
                    "permission_overwrites": [
                        overwrite(10, 0, "0", "2048")
                    ]
                },
                {
                    "id": "21",
                    "type": 11,
                    "parent_id": "20",
                    "name": "a thread"
                }
            ],
            "threads": [
                {"id": "21", "type": 11, "parent_id": "20", "name": "a thread"}
            ],
            "members": []
        }));
        let member = member(&[10]);

        let parent = guild.channels.get(&ChannelId::new(20)).expect("parent");
        let thread = guild.channels.get(&ChannelId::new(21)).expect("thread");

        let parent_permissions = guild.permissions_for(&member, parent);
        let thread_permissions = guild.permissions_for(&member, thread);

        assert!(!parent_permissions.contains(Permissions::SEND_MESSAGES));
        assert_eq!(thread_permissions, parent_permissions);
        assert!(thread_permissions.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn thread_without_a_cached_parent_uses_its_own_overwrites() {
        let guild = simple_guild();
        let member = member(&[10]);
        let thread: Channel = serde_json::from_value(serde_json::json!({
            "id": "21",
            "type": 11,
            "parent_id": "999",
            "name": "orphan thread",
            "permission_overwrites": [overwrite(10, 0, "0", "1024")]
        }))
        .expect("thread fixture");

        let permissions = guild.permissions_for(&member, &thread);

        assert!(!permissions.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn member_permissions_ignore_channel_overwrites() {
        let guild = simple_guild();
        let member = member(&[10]);

        assert_eq!(
            guild.member_permissions(&member),
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES
        );
    }

    #[test]
    fn a_role_the_member_does_not_have_is_ignored() {
        let guild = simple_guild();
        let member = member(&[]);
        let channel = channel_with_overwrites(&serde_json::json!([overwrite(10, 0, "0", "1024")]));

        let permissions = guild.permissions_for(&member, &channel);

        assert!(permissions.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn guild_without_an_everyone_role_uses_empty_base_permissions() {
        let guild = guild(serde_json::json!({
            "id": "1",
            "name": "Test Guild",
            "owner_id": "100",
            "roles": [],
            "members": []
        }));
        let member = member(&[]);
        let channel = channel_with_overwrites(&serde_json::json!([]));

        assert_eq!(
            guild.permissions_for(&member, &channel),
            Permissions::empty()
        );
    }

    #[test]
    fn unknown_permission_overwrite_fields_are_ignored() {
        let guild = simple_guild();
        let member = member(&[10]);
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "id": "20",
            "type": 0,
            "permission_overwrites": [{
                "id": "10",
                "type": 0,
                "allow": "1024",
                "deny": "0",
                "future_field": {"nested": true}
            }]
        }))
        .expect("channel fixture");

        let permissions = guild.permissions_for(&member, &channel);

        assert!(permissions.contains(Permissions::VIEW_CHANNEL));
    }

    #[test]
    fn serialising_a_guild_emits_roles_chunks_as_arrays_again() {
        let guild = simple_guild();

        let json = serde_json::to_value(&guild).expect("serialisable");

        assert_eq!(json["roles"].as_array().map(Vec::len), Some(2));
        assert_eq!(json["channels"].as_array().map(Vec::len), Some(1));
        assert_eq!(json["members"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn malformed_members_are_skipped_instead_of_failing_the_guild() {
        let guild = guild(serde_json::json!({
            "id": "1",
            "name": "Test Guild",
            "owner_id": "100",
            "roles": [],
            "members": [
                {"user": {"id": "200", "username": "ok"}, "roles": [], "deaf": false, "mute": false},
                {"nick": "this member has no user object"}
            ]
        }));

        assert_eq!(guild.members.len(), 1);
        assert!(guild.members.contains_key(&UserId::new(200)));
    }
}
