use bot_core::{CmdContext, EvtContext, OptionExt as _, UserData, With};
use eyre::{OptionExt as _, Result, WrapErr as _, ensure};
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::all::{
    Builder, ButtonStyle, ComponentInteraction, ComponentInteractionDataKind, CreateActionRow,
    CreateButton, CreateInteractionResponse, CreateInteractionResponseMessage, EditMessage,
    Message, MessageId, ReactionType, Role, RoleId,
};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SHOW_ROLE_SELECTION_ID: &str = "show_role_selection";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigT {
    buttons: BTreeMap<MessageId, RoleButtonData>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
struct RoleButtonData {
    on_click: Option<RoleId>,
    roles: Vec<RoleData>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
struct RoleData {
    role_id: serenity::RoleId,
    description: String,
    #[serde(with = "bot_core::serde::emoji")]
    emoji: ReactionType,
}

/// Manage role buttons
#[poise::command(
    slash_command,
    subcommands("new", "insert", "remove", "on_click"),
    subcommand_required = true,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn role_button<D: With<ConfigT>>(_ctx: CmdContext<'_, D>) -> Result<()> {
    Ok(())
}

/// Add a new role button to a bot message
#[poise::command(slash_command, guild_only)]
pub async fn new<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Link to a message sent by this bot"] bot_message: Message,
) -> Result<()> {
    ensure!(bot_message.author.id == ctx.framework().bot_id(), "That message wasn't sent by me");

    ctx.data()
        .with_mut_ok(|cfg| {
            cfg.buttons.insert(bot_message.id, RoleButtonData { on_click: None, roles: vec![] });
        })
        .await?;

    EditMessage::new()
        .components(vec![CreateActionRow::Buttons(vec![
            CreateButton::new(SHOW_ROLE_SELECTION_ID)
                .label("Select Roles")
                .style(ButtonStyle::Primary),
        ])])
        .execute(ctx, (bot_message.channel_id, bot_message.id, None))
        .await?;

    ctx.say("Role button added").await?;
    Ok(())
}

/// Insert a role into an existing role button's list
#[poise::command(slash_command)]
pub async fn insert<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Link to a message with a role button"] message_with_button: Message,
    #[description = "Role to add"] role: Role,
    #[description = "Description of the role"] description: String,
    #[description = "Emoji to use for the role"] emoji: ReactionType,
) -> Result<()> {
    let role_data = RoleData { role_id: role.id, description, emoji };

    ctx.data()
        .with_mut(|cfg| {
            let role_button =
                cfg.buttons.get_mut(&message_with_button.id).ok_or_eyre("Unknown role button")?;
            let roles = &mut role_button.roles;

            if let Some(role_index) = roles.iter().position(|r| r.role_id == role.id) {
                roles[role_index] = role_data.clone();
            } else {
                roles.push(role_data.clone());
            }

            Ok(())
        })
        .await?;

    ctx.say(format!("Added role to button: {role_data:?}")).await?;
    Ok(())
}

/// Remove a role from an existing role button's list
#[poise::command(slash_command)]
pub async fn remove<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Link to a message with a role button"] message_with_button: Message,
    #[description = "Role to remove"] role: Role,
) -> Result<()> {
    let role_data = ctx
        .data()
        .with_mut(|cfg| {
            let role_button =
                cfg.buttons.get_mut(&message_with_button.id).ok_or_eyre("Unknown role button")?;
            let roles = &mut role_button.roles;

            let role_index = roles
                .iter()
                .position(|r| r.role_id == role.id)
                .ok_or_eyre("The role is not configured for that role button")?;

            Ok(roles.remove(role_index))
        })
        .await?;

    ctx.say(format!("Removed role: {role_data:?}")).await?;
    Ok(())
}

/// Set a role that you get when you click the button
#[poise::command(slash_command)]
pub async fn on_click<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Link to a message with a role button"] message_with_button: Message,
    #[description = "Role to get on click"] role: Option<Role>,
) -> Result<()> {
    let role = role.as_ref();
    let role_id = ctx
        .data()
        .with_mut(|cfg| {
            let role_button =
                cfg.buttons.get_mut(&message_with_button.id).ok_or_eyre("Unknown role button")?;
            role_button.on_click = role.map(|r| r.id);
            Ok(role.map(|r| r.id))
        })
        .await?;

    ctx.say(format!("Added on_click role to button: {role_id:?}")).await?;
    Ok(())
}

pub async fn show_role_selection(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    int: &ComponentInteraction,
) -> Result<()> {
    let button_message_id = int.message.id;
    let guild_id = int.guild_id.ok_or_eyre("No guild")?;
    let guild_roles = guild_id.to_guild_cached(ctx.serenity_context).some()?.roles.clone();

    let initial_response = {
        let role_button = read_role_button_data(ctx.user_data, &button_message_id).await?;
        let member =
            guild_id.member(ctx.serenity_context, int.user.id).await.wrap_err("No member")?;

        if let Some(on_click_role) = role_button.on_click {
            member.add_role(ctx.serenity_context, on_click_role).await?;
        }

        if role_button.roles.is_empty() {
            int.create_response(
                ctx.serenity_context,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("No roles have been configured for this button")
                        .ephemeral(true),
                ),
            )
            .await?;
            return Ok(());
        }

        int.create_response(
            ctx.serenity_context,
            CreateInteractionResponse::Message(role_selection_message(
                &guild_roles,
                &member.roles.iter().collect(),
                role_button.roles,
            )?),
        )
        .await?;
        int.get_response(ctx.serenity_context).await?
    };

    while let Some(int) = initial_response.await_component_interaction(ctx.serenity_context).await {
        let ComponentInteractionDataKind::StringSelect { values } = int.data.kind.clone() else {
            tracing::error!("Unexpected interaction kind: {:?}", int.data.kind);
            continue;
        };

        let role_button = read_role_button_data(ctx.user_data, &button_message_id).await?;
        let selectable: HashSet<_> = role_button.roles.iter().map(|r| r.role_id).collect();

        let selected: HashSet<_> = values.into_iter().filter_map(|s| s.parse().ok()).collect();
        let selected: HashSet<_> = selected.intersection(&selectable).collect();

        let member =
            guild_id.member(ctx.serenity_context, int.user.id).await.wrap_err("No member")?;
        let current: HashSet<_> = member.roles.iter().cloned().collect();
        let current: HashSet<_> = current.intersection(&selectable).collect();

        for &role_id in selected.difference(&current) {
            member.add_role(ctx.serenity_context, role_id).await?;
        }
        for &role_id in current.difference(&selected) {
            member.remove_role(ctx.serenity_context, role_id).await?;
        }

        int.create_response(
            ctx.serenity_context,
            CreateInteractionResponse::UpdateMessage(role_selection_message(
                &guild_roles,
                &selected,
                role_button.roles,
            )?),
        )
        .await?;
    }

    Ok(())
}

async fn read_role_button_data(
    data: &impl With<ConfigT>,
    message_id: &MessageId,
) -> Result<RoleButtonData> {
    data.with(|cfg| Ok(cfg.buttons.get(message_id).ok_or_eyre("Unknown role button")?.clone()))
        .await
}

fn role_selection_message(
    guild_roles: &HashMap<RoleId, Role>,
    member_roles: &HashSet<&RoleId>,
    selectable_roles: impl IntoIterator<Item = RoleData>,
) -> Result<CreateInteractionResponseMessage> {
    let options: Vec<_> = selectable_roles
        .into_iter()
        .filter_map(|role| {
            Some(
                serenity::CreateSelectMenuOption::new(
                    guild_roles.get(&role.role_id)?.name.clone(),
                    role.role_id.get().to_string(),
                )
                .description(role.description)
                .emoji(role.emoji)
                .default_selection(member_roles.contains(&role.role_id)),
            )
        })
        .collect();
    let max_values = options.len() as u8;

    Ok(CreateInteractionResponseMessage::new()
        .components(vec![serenity::CreateActionRow::SelectMenu(
            serenity::CreateSelectMenu::new(
                "roles",
                serenity::CreateSelectMenuKind::String { options },
            )
            .min_values(0)
            .max_values(max_values),
        )])
        .ephemeral(true))
}
