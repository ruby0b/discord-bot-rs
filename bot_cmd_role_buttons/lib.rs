use bot_core::choice_parameters::ButtonStyleParameter;
use bot_core::ext::create_reply::CreateReplyExt;
use bot_core::ext::option::OptionExt as _;
use bot_core::{CmdContext, EvtContext, UserData, With};
use eyre::{OptionExt as _, Result, bail, ensure};
use poise::CreateReply;
use poise::serenity_prelude::all::{
    Builder, ButtonStyle, ComponentInteraction, ComponentInteractionDataKind, CreateActionRow, CreateButton,
    EditMessage, Message, MessageId, ReactionType, Role, RoleId,
};
use poise::serenity_prelude::{self as serenity, CreateSelectMenuOption};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SHOW_ROLE_SELECTION_ID: &str = "show_role_selection";
pub const ROLE_BUTTON_SELECT_ID: &str = "role_buttons.select";

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
    #[description = "Button label"] button_label: Option<String>,
    #[description = "Button style"] button_style: Option<ButtonStyleParameter>,
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
                .label(button_label.as_ref().map_or("Select Roles", |s| s))
                .style(button_style.map_or(ButtonStyle::Primary, |s| s.into())),
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
            let role_button = cfg.buttons.get_mut(&message_with_button.id).ok_or_eyre("Unknown role button")?;
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
            let role_button = cfg.buttons.get_mut(&message_with_button.id).ok_or_eyre("Unknown role button")?;
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
            let role_button = cfg.buttons.get_mut(&message_with_button.id).ok_or_eyre("Unknown role button")?;
            role_button.on_click = role.map(|r| r.id);
            Ok(role.map(|r| r.id))
        })
        .await?;

    ctx.say(format!("Added on_click role to button: {role_id:?}")).await?;
    Ok(())
}

pub async fn show_role_selection(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    interaction: &ComponentInteraction,
) -> Result<()> {
    interaction.defer_ephemeral(ctx.serenity_context).await?;

    let guild_id = interaction.guild_id.ok_or_eyre("No guild")?;
    let user_id = interaction.user.id;
    let role_names: HashMap<RoleId, String> = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        guild.roles.iter().map(|(&id, role)| (id, role.name.clone())).collect()
    };

    let button_config = read_role_button_data(ctx.user_data, &interaction.message.id).await?;

    if let Some(on_click_role) = button_config.on_click {
        ctx.serenity_context.http.add_member_role(guild_id, user_id, on_click_role, Some("Button on-click")).await?;
    }

    ensure!(!button_config.roles.is_empty(), "No roles have been configured for this button");

    let roles: HashSet<RoleId> = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        guild.members.get(&user_id).ok_or_eyre("Unknown member")?.roles.iter().copied().collect()
    };
    role_selection_message(&role_names, &roles, button_config.roles)?
        .edit_original_response(ctx.serenity_context, interaction)
        .await?;

    Ok(())
}

pub async fn submit_role_selection(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    interaction: &ComponentInteraction,
) -> Result<()> {
    interaction.defer_ephemeral(ctx.serenity_context).await?;

    let ComponentInteractionDataKind::StringSelect { ref values } = interaction.data.kind else {
        bail!("Unexpected interaction kind: {:?}", interaction.data.kind);
    };

    let guild_id = interaction.guild_id.ok_or_eyre("No guild")?;
    let user_id = interaction.user.id;
    let all_current_roles: HashSet<RoleId> = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        guild.members.get(&interaction.user.id).ok_or_eyre("Unknown member")?.roles.iter().copied().collect()
    };

    let button_config = read_role_button_data(ctx.user_data, &interaction.message.id).await?;
    let selectable_roles: HashSet<RoleId> = button_config.roles.iter().map(|r| r.role_id).collect();

    let selected_roles: HashSet<RoleId> = values.iter().filter_map(|s| s.parse().ok()).collect();
    let selected_roles: HashSet<RoleId> = selected_roles.intersection(&selectable_roles).copied().collect();

    let current_roles: HashSet<RoleId> = all_current_roles.intersection(&selectable_roles).copied().collect();

    for &role_id in selected_roles.difference(&current_roles) {
        ctx.serenity_context.http.add_member_role(guild_id, user_id, role_id, Some("Button")).await?;
    }
    for &role_id in current_roles.difference(&selected_roles) {
        ctx.serenity_context.http.remove_member_role(guild_id, user_id, role_id, Some("Button")).await?;
    }

    let roles: HashMap<RoleId, String> = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        guild.roles.iter().map(|(&id, role)| (id, role.name.clone())).collect()
    };
    role_selection_message(&roles, &selected_roles, button_config.roles)?
        .edit_original_response(ctx.serenity_context, interaction)
        .await?;

    interaction.message.delete(ctx.serenity_context).await?;

    Ok(())
}

async fn read_role_button_data(data: &impl With<ConfigT>, message_id: &MessageId) -> Result<RoleButtonData> {
    data.with(|cfg| Ok(cfg.buttons.get(message_id).ok_or_eyre("Unknown role button")?.clone())).await
}

fn role_selection_message(
    role_names: &HashMap<RoleId, String>,
    member_roles: &HashSet<RoleId>,
    selectable_roles: impl IntoIterator<Item = RoleData>,
) -> Result<CreateReply> {
    let options: Vec<CreateSelectMenuOption> = selectable_roles
        .into_iter()
        .filter_map(|role| {
            Some(
                serenity::CreateSelectMenuOption::new(
                    role_names.get(&role.role_id)?.clone(),
                    role.role_id.get().to_string(),
                )
                .description(role.description)
                .emoji(role.emoji)
                .default_selection(member_roles.contains(&role.role_id)),
            )
        })
        .collect();
    let max_values = options.len() as u8;

    Ok(CreateReply::new()
        .components(vec![serenity::CreateActionRow::SelectMenu(
            serenity::CreateSelectMenu::new(ROLE_BUTTON_SELECT_ID, serenity::CreateSelectMenuKind::String { options })
                .min_values(0)
                .max_values(max_values),
        )])
        .ephemeral(true))
}
