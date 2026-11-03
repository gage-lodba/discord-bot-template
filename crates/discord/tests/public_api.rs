//! Integration tests that exercise the crate the way a downstream user does:
//! only through the public API, with wire payloads taken from the documented
//! Discord shapes. Unit tests live next to the code; these guard the public
//! surface and the documented JSON contracts end to end.
//!
//! Reference: `docs/reference.md` (ID/timestamp formats),
//! `docs/permissions.md` (overwrite hierarchy),
//! `docs/receiving-and-responding.md` (interaction payloads, callback types and
//! resolved data), `docs/components-reference.md` (component objects),
//! `docs/gateway-events.md` (message payloads), `docs/channel.md` (message
//! request body).

use discord::builder::attachment::AttachmentBuilder;
use discord::builder::command::CommandBuilder;
use discord::builder::command::CommandOptionBuilder;
use discord::builder::component::ContainerBuilder;
use discord::builder::component::FileUploadBuilder;
use discord::builder::component::GroupOptionBuilder;
use discord::builder::component::RadioGroupBuilder;
use discord::builder::component::TextDisplayBuilder;
use discord::builder::interaction::ActionRowBuilder;
use discord::builder::interaction::AutocompleteBuilder;
use discord::builder::interaction::ButtonBuilder;
use discord::builder::interaction::DeferUpdateBuilder;
use discord::builder::interaction::InteractionResponse;
use discord::builder::interaction::LabelBuilder;
use discord::builder::interaction::ModalBuilder;
use discord::builder::interaction::ReplyBuilder;
use discord::builder::interaction::SelectOptionBuilder;
use discord::builder::interaction::StringSelectBuilder;
use discord::builder::interaction::TextInputBuilder;
use discord::builder::interaction::UpdateMessageBuilder;
use discord::builder::message::MessageBuilder;
use discord::builder::poll::PollBuilder;
use discord::id::ChannelId;
use discord::id::GuildId;
use discord::id::RoleId;
use discord::id::UserId;
use discord::intents::GatewayIntents;
use discord::model::application::CommandOptionKind;
use discord::model::application::Interaction;
use discord::model::application::InteractionCallbackType;
use discord::model::application::InteractionType;
use discord::model::component::Component;
use discord::model::guild::Guild;
use discord::model::message::Message;
use discord::model::message::MessageFlags;
use discord::permissions::Permissions;
use discord::timestamp::Timestamp;

/// Builds a guild payload in the shape `GUILD_CREATE` delivers.
fn guild_payload() -> serde_json::Value {
    serde_json::json!({
        "id": "1",
        "name": "Test Guild",
        "owner_id": "100",
        "roles": [
            {
                "id": "1",
                "name": "@everyone",
                "color": 0,
                "hoist": false,
                "position": 0,
                "permissions": "1024", // VIEW_CHANNEL
                "managed": false,
                "mentionable": false
            },
            {
                "id": "10",
                "name": "Member",
                "color": 3_447_003,
                "hoist": true,
                "position": 1,
                "permissions": "2048", // SEND_MESSAGES
                "managed": false,
                "mentionable": true
            }
        ],
        "channels": [
            {"id": "20", "type": 0, "guild_id": "1", "name": "general"},
            {
                "id": "21",
                "type": 0,
                "guild_id": "1",
                "name": "read-only",
                "permission_overwrites": [
                    {"id": "10", "type": 0, "allow": "0", "deny": "2048"}
                ]
            },
            {
                "id": "22",
                "type": 0,
                "guild_id": "1",
                "name": "admin-only",
                "permission_overwrites": [
                    {"id": "1", "type": 0, "allow": "0", "deny": "1024"}
                ]
            }
        ],
        "members": [
            {
                "user": {"id": "200", "username": "member", "discriminator": "0"},
                "nick": null,
                "roles": ["10"],
                "joined_at": "2020-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false
            },
            {
                "user": {"id": "100", "username": "owner", "discriminator": "0"},
                "nick": null,
                "roles": [],
                "joined_at": "2020-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false
            }
        ]
    })
}

/// Parses a documented application-command interaction payload.
fn command_interaction_payload() -> serde_json::Value {
    serde_json::json!({
        "type": 2,
        "id": "100",
        "application_id": "200",
        "token": "interaction-token",
        "guild_id": "300",
        "channel_id": "400",
        "member": {
            "user": {"id": "500", "username": "invoker", "discriminator": "0"},
            "nick": "Invoker",
            "roles": ["10"],
            "joined_at": "2020-01-01T00:00:00.000000+00:00",
            "deaf": false,
            "mute": false,
            "permissions": "2048"
        },
        "data": {
            "id": "700",
            "name": "ban",
            "type": 1,
            "options": [
                {"name": "target", "type": 6, "value": "500"},
                {"name": "duration", "type": 4, "value": 3},
                {"name": "reason", "type": 3, "value": "spam"}
            ]
        },
        "app_permissions": "2048",
        "locale": "en-US",
        "guild_locale": "en-US"
    })
}

#[test]
fn command_registration_serialises_the_documented_body() {
    let command = CommandBuilder::new("ban", "Bans a user from the guild.")
        .add_option(
            CommandOptionBuilder::new(
                CommandOptionKind::User,
                "target",
                "Bans the user from the guild.",
            )
            .required(true),
        )
        .add_option(
            CommandOptionBuilder::new(CommandOptionKind::Integer, "duration", "Days 0-7.")
                .min_int_value(0)
                .max_int_value(7),
        )
        .default_member_permissions(Permissions::BAN_MEMBERS)
        .to_json();

    // `default_member_permissions` is a decimal string, option `type` is the
    // documented integer and min/max are numbers.
    assert_eq!(
        command,
        serde_json::json!({
            "name": "ban",
            "description": "Bans a user from the guild.",
            "default_member_permissions": "4",
            "options": [
                {
                    "type": 6,
                    "name": "target",
                    "description": "Bans the user from the guild.",
                    "required": true
                },
                {
                    "type": 4,
                    "name": "duration",
                    "description": "Days 0-7.",
                    "min_value": 0,
                    "max_value": 7
                }
            ]
        })
    );
}

#[test]
fn interaction_payload_parses_and_exposes_options() {
    let interaction: Interaction = serde_json::from_value(command_interaction_payload())
        .expect("documented interaction shape");

    let Interaction::Command(command) = interaction else {
        panic!("expected an application command interaction");
    };

    assert_eq!(command.data.name, "ban");
    assert_eq!(command.user.id, UserId::new(500));
    assert_eq!(command.user.username, "invoker");
    assert_eq!(command.guild_id, Some(GuildId::new(300)));
    assert_eq!(command.channel_id, Some(ChannelId::new(400)));
    assert_eq!(
        command.data.options[0].value.as_user_id(),
        Some(UserId::new(500))
    );
    assert_eq!(command.data.options[1].value.as_i64(), Some(3));
    assert_eq!(command.data.options[2].value.as_str(), Some("spam"));
    assert_eq!(
        command
            .member
            .as_ref()
            .map(discord::model::guild::Member::display_name),
        Some("Invoker")
    );
}

#[test]
fn permission_hierarchy_honours_role_and_channel_overwrites() {
    let guild: Guild = serde_json::from_value(guild_payload()).expect("documented guild shape");
    let member = guild
        .members
        .get(&UserId::new(200))
        .expect("member is cached");
    let owner = guild
        .members
        .get(&UserId::new(100))
        .expect("owner is cached");

    let general = guild
        .channels
        .get(&ChannelId::new(20))
        .expect("channel is cached");
    let read_only = guild
        .channels
        .get(&ChannelId::new(21))
        .expect("channel is cached");
    let admin_only = guild
        .channels
        .get(&ChannelId::new(22))
        .expect("channel is cached");

    // Base roles: @everyone VIEW_CHANNEL plus the member role's SEND_MESSAGES.
    let permissions = guild.permissions_for(member, general);
    assert!(permissions.contains(Permissions::VIEW_CHANNEL));
    assert!(permissions.contains(Permissions::SEND_MESSAGES));

    // A role overwrite denying SEND_MESSAGES wins over the role grant.
    assert!(
        !guild
            .permissions_for(member, read_only)
            .contains(Permissions::SEND_MESSAGES)
    );

    // The @everyone overwrite removes VIEW_CHANNEL from the base set.
    assert!(
        !guild
            .permissions_for(member, admin_only)
            .contains(Permissions::VIEW_CHANNEL)
    );

    // The guild owner bypasses channel overwrites entirely.
    assert!(
        guild
            .permissions_for(owner, admin_only)
            .contains(Permissions::VIEW_CHANNEL)
    );
}

#[test]
fn message_parses_from_and_serialises_to_the_documented_shape() {
    let payload = serde_json::json!({
        "id": "1000",
        "channel_id": "20",
        "guild_id": "1",
        "author": {"id": "200", "username": "member", "discriminator": "0", "bot": false},
        "content": "hello from the wire",
        "timestamp": "2026-10-22T17:00:00.000000+00:00",
        "pinned": false
    });

    let message: Message = serde_json::from_value(payload).expect("documented message shape");

    assert_eq!(message.content, "hello from the wire");
    assert_eq!(message.author.id, UserId::new(200));
    assert_eq!(message.channel_id, ChannelId::new(20));
    assert_eq!(message.guild_id, Some(GuildId::new(1)));
    assert_eq!(
        message.timestamp,
        Timestamp::from_unix_millis(1_792_688_400_000)
    );
}

#[test]
fn interaction_reply_serialises_the_callback_shape() {
    let response: InteractionResponse = ReplyBuilder::new().content("Pong!").ephemeral().into();

    assert_eq!(
        response.to_json(),
        serde_json::json!({"type": 4, "data": {"content": "Pong!", "flags": 64}})
    );
}

#[test]
fn intent_bits_match_the_documented_identify_value() {
    let intents = GatewayIntents::GUILDS.union(GatewayIntents::GUILD_MODERATION);

    // docs/gateway-events.md: GUILDS is 1 << 0 and GUILD_MODERATION is 1 << 2.
    assert_eq!(intents.bits(), 5);
    assert_eq!(
        serde_json::to_value(intents).expect("serialisable"),
        serde_json::json!(5)
    );
}

//
//  Interactions: the complete documented surface
//

/// The documented guild member object, reused by the interaction fixtures.
fn member_payload() -> serde_json::Value {
    serde_json::json!({
        "user": {"id": "500", "username": "invoker", "discriminator": "0"},
        "nick": "Invoker",
        "roles": ["600"],
        "joined_at": "2020-01-01T00:00:00.000000+00:00",
        "deaf": false,
        "mute": false,
        "permissions": "1024"
    })
}

/// A documented `MESSAGE_COMPONENT` interaction payload, including the message
/// the component was attached to and resolved entities.
fn component_interaction_payload() -> serde_json::Value {
    serde_json::json!({
        "type": 3,
        "id": "10",
        "application_id": "2",
        "token": "interaction-token",
        "version": 1,
        "guild_id": "300",
        "channel_id": "400",
        "member": member_payload(),
        "message": {
            "id": "1000",
            "channel_id": "400",
            "guild_id": "300",
            "author": {"id": "500", "username": "invoker", "discriminator": "0"},
            "content": "original message",
            "timestamp": "2026-10-22T17:00:00.000000+00:00",
            "components": [
                {
                    "type": 1,
                    "components": [
                        {"type": 2, "style": 1, "label": "Click", "custom_id": "btn"}
                    ]
                }
            ]
        },
        "data": {
            "custom_id": "btn",
            "component_type": 2,
            "values": [],
            "resolved": {
                "users": {
                    "500": {"id": "500", "username": "invoker", "discriminator": "0"}
                },
                "roles": {
                    "600": {
                        "id": "600",
                        "name": "Member",
                        "color": 0,
                        "hoist": false,
                        "position": 1,
                        "permissions": "1024",
                        "managed": false,
                        "mentionable": false
                    }
                },
                "channels": {
                    "700": {"id": "700", "type": 0, "name": "general"}
                }
            }
        }
    })
}

/// A documented `APPLICATION_COMMAND_AUTOCOMPLETE` interaction payload with a
/// focused option.
fn autocomplete_interaction_payload() -> serde_json::Value {
    serde_json::json!({
        "type": 4,
        "id": "11",
        "application_id": "2",
        "token": "interaction-token",
        "version": 1,
        "guild_id": "300",
        "channel_id": "400",
        "member": member_payload(),
        "data": {
            "id": "30",
            "name": "search",
            "type": 1,
            "options": [
                {"name": "query", "type": 3, "value": "C", "focused": true}
            ]
        }
    })
}

/// A documented `MODAL_SUBMIT` interaction payload with the submitted label and
/// text input values.
fn modal_submit_payload() -> serde_json::Value {
    serde_json::json!({
        "type": 5,
        "id": "12",
        "application_id": "2",
        "token": "interaction-token",
        "version": 1,
        "guild_id": "300",
        "channel_id": "400",
        "member": member_payload(),
        "data": {
            "custom_id": "bug_modal",
            "components": [
                {
                    "type": 18,
                    "component": {
                        "type": 4,
                        "custom_id": "details",
                        "value": "it broke"
                    }
                }
            ]
        }
    })
}

#[test]
fn all_five_documented_interaction_types_parse() {
    let ping: Interaction = serde_json::from_value(serde_json::json!({
        "type": 1,
        "id": "1",
        "application_id": "2",
        "token": "interaction-token",
        "version": 1
    }))
    .expect("documented ping shape");
    assert!(matches!(ping, Interaction::Ping(_)));
    assert_eq!(ping.kind(), InteractionType::Ping);

    let command: Interaction =
        serde_json::from_value(command_interaction_payload()).expect("documented command shape");
    assert_eq!(command.kind(), InteractionType::ApplicationCommand);

    let component: Interaction = serde_json::from_value(component_interaction_payload())
        .expect("documented component shape");
    assert_eq!(component.kind(), InteractionType::MessageComponent);

    let autocomplete: Interaction = serde_json::from_value(autocomplete_interaction_payload())
        .expect("documented autocomplete shape");
    assert_eq!(
        autocomplete.kind(),
        InteractionType::ApplicationCommandAutocomplete
    );

    let modal: Interaction =
        serde_json::from_value(modal_submit_payload()).expect("documented modal submit shape");
    assert_eq!(modal.kind(), InteractionType::ModalSubmit);

    let unknown: Interaction =
        serde_json::from_value(serde_json::json!({"type": 99})).expect("unknown type");
    assert_eq!(unknown.kind(), InteractionType::Unknown(99));
}

#[test]
fn autocomplete_interaction_exposes_the_focused_option() {
    let interaction: Interaction = serde_json::from_value(autocomplete_interaction_payload())
        .expect("documented autocomplete shape");
    let Interaction::Autocomplete(autocomplete) = interaction else {
        panic!("expected an autocomplete interaction");
    };

    let focused = autocomplete
        .focused_option()
        .expect("the focused option is present");
    assert_eq!(focused.name, "query");
    assert_eq!(focused.value.as_str(), Some("C"));
    assert!(focused.focused);
}

#[test]
fn component_interaction_exposes_message_and_resolved_data() {
    let interaction: Interaction = serde_json::from_value(component_interaction_payload())
        .expect("documented component shape");
    let Interaction::Component(component) = interaction else {
        panic!("expected a component interaction");
    };

    assert_eq!(component.data.custom_id, "btn");
    assert_eq!(component.data.values, Vec::<String>::new());

    let message = component.message.expect("the component message is present");
    assert_eq!(message.content, "original message");
    let Component::ActionRow(row) = &message.components[0] else {
        panic!("expected a typed action row");
    };
    let Component::Button(button) = &row.components[0] else {
        panic!("expected a typed button");
    };
    assert_eq!(button.custom_id.as_deref(), Some("btn"));

    let resolved = component.data.resolved.expect("resolved data is present");
    assert_eq!(
        resolved
            .user(UserId::new(500))
            .map(|user| user.username.as_str()),
        Some("invoker")
    );
    assert_eq!(
        resolved
            .role(RoleId::new(600))
            .map(|role| role.name.as_str()),
        Some("Member")
    );
    assert!(resolved.channel(ChannelId::new(700)).is_some());
}

#[test]
fn modal_submit_interaction_parses_submitted_values() {
    let interaction: Interaction =
        serde_json::from_value(modal_submit_payload()).expect("documented modal submit shape");
    let Interaction::ModalSubmit(modal) = interaction else {
        panic!("expected a modal submit interaction");
    };

    assert_eq!(modal.data.custom_id, "bug_modal");
    let Component::Label(label) = &modal.data.components[0] else {
        panic!("expected a typed label");
    };
    let Component::TextInput(input) = label.component.as_ref() else {
        panic!("expected a typed text input");
    };
    assert_eq!(input.custom_id, "details");
    assert_eq!(input.value.as_deref(), Some("it broke"));
}

#[test]
fn interaction_callback_types_cover_the_documented_table() {
    for (value, expected) in [
        (1, InteractionCallbackType::Pong),
        (4, InteractionCallbackType::ChannelMessageWithSource),
        (5, InteractionCallbackType::DeferredChannelMessageWithSource),
        (6, InteractionCallbackType::DeferredUpdateMessage),
        (7, InteractionCallbackType::UpdateMessage),
        (
            8,
            InteractionCallbackType::ApplicationCommandAutocompleteResult,
        ),
        (9, InteractionCallbackType::Modal),
        (10, InteractionCallbackType::PremiumRequired),
        (12, InteractionCallbackType::LaunchActivity),
    ] {
        assert_eq!(InteractionCallbackType::from_i64(value), expected);
        assert_eq!(expected.as_i64(), value);
    }

    assert_eq!(
        InteractionCallbackType::from_i64(99),
        InteractionCallbackType::Unknown(99)
    );
}

#[test]
fn callback_builders_serialise_every_documented_type() {
    assert_eq!(
        InteractionResponse::Pong.to_json(),
        serde_json::json!({"type": 1})
    );
    assert_eq!(
        InteractionResponse::PremiumRequired.to_json(),
        serde_json::json!({"type": 10})
    );
    assert_eq!(
        InteractionResponse::LaunchActivity.to_json(),
        serde_json::json!({"type": 12})
    );

    let defer_update: InteractionResponse = DeferUpdateBuilder::new().into();
    assert_eq!(defer_update.to_json(), serde_json::json!({"type": 6}));

    let update: InteractionResponse = UpdateMessageBuilder::new().content("updated").into();
    assert_eq!(
        update.to_json(),
        serde_json::json!({"type": 7, "data": {"content": "updated"}})
    );

    let autocomplete: InteractionResponse = AutocompleteBuilder::new()
        .choice("Animal: dog", "dog")
        .choice("Count: 7", 7_i64)
        .into();
    assert_eq!(
        autocomplete.to_json(),
        serde_json::json!({
            "type": 8,
            "data": {
                "choices": [
                    {"name": "Animal: dog", "value": "dog"},
                    {"name": "Count: 7", "value": 7}
                ]
            }
        })
    );

    let modal: InteractionResponse = ModalBuilder::new("bug_modal", "Bug report")
        .label(LabelBuilder::new("Details").text_input(TextInputBuilder::paragraph("details")))
        .into();
    assert_eq!(
        modal.to_json(),
        serde_json::json!({
            "type": 9,
            "data": {
                "custom_id": "bug_modal",
                "title": "Bug report",
                "components": [{
                    "type": 18,
                    "label": "Details",
                    "component": {"type": 4, "custom_id": "details", "style": 2}
                }]
            }
        })
    );
}

#[test]
fn replies_can_carry_the_documented_component_rows() {
    let reply: InteractionResponse = ReplyBuilder::new()
        .content("pick one")
        .component(ActionRowBuilder::new().button(ButtonBuilder::primary("click", "Click me")))
        .component(
            ActionRowBuilder::new().string_select(
                StringSelectBuilder::new("pick")
                    .placeholder("Choose")
                    .option(SelectOptionBuilder::new("Ant", "ant")),
            ),
        )
        .into();

    assert_eq!(
        reply.to_json(),
        serde_json::json!({
            "type": 4,
            "data": {
                "content": "pick one",
                "components": [
                    {
                        "type": 1,
                        "components": [{
                            "type": 2,
                            "style": 1,
                            "label": "Click me",
                            "custom_id": "click"
                        }]
                    },
                    {
                        "type": 1,
                        "components": [{
                            "type": 3,
                            "custom_id": "pick",
                            "placeholder": "Choose",
                            "options": [{"label": "Ant", "value": "ant"}]
                        }]
                    }
                ]
            }
        })
    );
}

#[test]
fn command_options_can_request_autocomplete() {
    let enabled = CommandOptionBuilder::new(CommandOptionKind::Str, "query", "Search text")
        .autocomplete(true)
        .to_json();
    assert_eq!(enabled["autocomplete"], serde_json::json!(true));

    let unset = CommandOptionBuilder::new(CommandOptionKind::Str, "query", "Search text").to_json();
    assert_eq!(unset.get("autocomplete"), None);
}

#[test]
fn message_components_and_interaction_metadata_parse() {
    let message: Message = serde_json::from_value(serde_json::json!({
        "id": "1000",
        "channel_id": "400",
        "guild_id": "300",
        "author": {"id": "500", "username": "invoker", "discriminator": "0"},
        "content": "a component message",
        "timestamp": "2026-10-22T17:00:00.000000+00:00",
        "components": [{
            "type": 1,
            "components": [{"type": 2, "style": 2, "label": "Secondary", "custom_id": "sec"}]
        }],
        "interaction": {
            "id": "10",
            "type": 2,
            "name": "interact-test",
            "user": {"id": "500", "username": "invoker", "discriminator": "0"}
        }
    }))
    .expect("documented message shape");

    assert_eq!(message.components.len(), 1);
    assert_eq!(
        message.components.first().map(Component::kind),
        Some(discord::model::component::ComponentKind::ActionRow)
    );
    let interaction = message.interaction.expect("message interaction");
    assert_eq!(interaction.name, "interact-test");
    assert_eq!(interaction.kind, InteractionType::ApplicationCommand);
}

//
//  Message features: IS_COMPONENTS_V2, polls and file uploads.
//

#[test]
fn is_components_v2_messages_serialise_components_and_flag() {
    let body = MessageBuilder::new()
        .content("dropped for V2")
        .component(TextDisplayBuilder::new("# Hello from V2"))
        .component(
            ContainerBuilder::new().text_display(TextDisplayBuilder::new("inside a container")),
        )
        .to_json();

    assert_eq!(body["flags"], serde_json::json!(32_768));
    assert_eq!(body.get("content"), None);
    assert_eq!(
        body["components"][0],
        serde_json::json!({"type": 10, "content": "# Hello from V2"})
    );
    assert_eq!(body["components"][1]["type"], serde_json::json!(17));
    assert_eq!(
        body["components"][1]["components"][0]["type"],
        serde_json::json!(10)
    );
}

#[test]
fn is_components_v2_replies_serialise_components_and_flag() {
    let reply: InteractionResponse = ReplyBuilder::new()
        .content("dropped for V2")
        .component(TextDisplayBuilder::new("V2 reply"))
        .into();

    assert_eq!(
        reply.to_json(),
        serde_json::json!({
            "type": 4,
            "data": {
                "components": [{"type": 10, "content": "V2 reply"}],
                "flags": 32_768
            }
        })
    );
}

#[test]
fn update_message_v2_serialises_components_and_flag() {
    let update: InteractionResponse = UpdateMessageBuilder::new()
        .component(TextDisplayBuilder::new("edited as V2"))
        .into();

    assert_eq!(
        update.to_json(),
        serde_json::json!({
            "type": 7,
            "data": {
                "components": [{"type": 10, "content": "edited as V2"}],
                "flags": 32_768
            }
        })
    );
}

#[test]
fn modals_accept_the_v2_modal_inputs_through_labels() {
    let modal: InteractionResponse = ModalBuilder::new("v2_modal", "V2 modal")
        .label(LabelBuilder::new("Upload a file").file_upload(FileUploadBuilder::new("upload")))
        .label(LabelBuilder::new("Choose one").radio_group(
            RadioGroupBuilder::new("choice").option(GroupOptionBuilder::new("a", "Option A")),
        ))
        .into();

    let body = modal.to_json();

    assert_eq!(body["type"], serde_json::json!(9));
    assert_eq!(body["data"]["components"][0]["type"], serde_json::json!(18));
    assert_eq!(
        body["data"]["components"][0]["component"]["type"],
        serde_json::json!(19)
    );
    assert_eq!(
        body["data"]["components"][1]["component"]["type"],
        serde_json::json!(21)
    );
}

#[test]
fn polls_serialise_and_parse_from_the_documented_shape() {
    let body = MessageBuilder::new()
        .poll(
            PollBuilder::new("Deploy to production?")
                .answer("Yes")
                .answer("No")
                .duration_hours(24),
        )
        .to_json();

    assert_eq!(
        body["poll"]["question"]["text"],
        serde_json::json!("Deploy to production?")
    );
    assert_eq!(body["poll"]["answers"].as_array().map(Vec::len), Some(2));
    assert_eq!(body["poll"]["duration"], serde_json::json!(24));

    let message: Message = serde_json::from_value(serde_json::json!({
        "id": "1000",
        "channel_id": "20",
        "guild_id": "1",
        "author": {"id": "200", "username": "member", "discriminator": "0"},
        "content": "",
        "timestamp": "2026-10-22T17:00:00.000000+00:00",
        "poll": {
            "question": {"text": "Deploy to production?"},
            "answers": [
                {"answer_id": 1, "poll_media": {"text": "Yes"}},
                {"answer_id": 2, "poll_media": {"text": "No"}}
            ],
            "expiry": "2026-10-23T17:00:00.000000+00:00",
            "allow_multiselect": false,
            "layout_type": 1,
            "results": {"is_finalized": false, "answer_counts": []}
        }
    }))
    .expect("documented poll message shape");

    let poll = message.poll.expect("the poll is parsed");
    assert_eq!(poll.question.text.as_deref(), Some("Deploy to production?"));
    assert_eq!(poll.answers.len(), 2);
    assert_eq!(poll.answers[0].poll_media.text.as_deref(), Some("Yes"));
}

#[test]
fn file_uploads_carry_documented_attachment_metadata() {
    let message = MessageBuilder::new()
        .content("see attached")
        .attachment(AttachmentBuilder::new("report.txt", b"hello").with_description("a report"));

    let (json, files) = message.to_payload();

    assert_eq!(json["content"], serde_json::json!("see attached"));
    assert_eq!(
        json["attachments"],
        serde_json::json!([{
            "id": 0,
            "filename": "report.txt",
            "description": "a report"
        }])
    );
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].filename(), "report.txt");
    assert_eq!(files[0].data(), b"hello");
}

#[test]
fn message_flags_and_tts_are_typed_in_the_public_api() {
    let message: Message = serde_json::from_value(serde_json::json!({
        "id": "1000",
        "channel_id": "20",
        "author": {"id": "200", "username": "member", "discriminator": "0"},
        "content": "",
        "tts": true,
        "timestamp": "2026-10-22T17:00:00.000000+00:00",
        "flags": 32_768
    }))
    .expect("message with a documented flag bitfield");

    assert!(message.tts);
    assert!(message.is_components_v2());
    assert!(!message.is_ephemeral());
    assert!(message.flags.contains(MessageFlags::IS_COMPONENTS_V2));
    assert_eq!(
        message.flags,
        MessageFlags::IS_COMPONENTS_V2 | MessageFlags::empty()
    );
}

#[test]
fn command_options_accept_documented_choices() {
    let option = CommandOptionBuilder::new(CommandOptionKind::Str, "execute", "Add or remove.")
        .choice("Add", "add")
        .choice("Remove", "remove");

    assert_eq!(
        option.to_json()["choices"],
        serde_json::json!([
            {"name": "Add", "value": "add"},
            {"name": "Remove", "value": "remove"}
        ])
    );
}
