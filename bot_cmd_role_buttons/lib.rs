use bot_core::ext::create_reply::CreateReplyExt;
use bot_core::ext::option::OptionExt as _;
use bot_core::{EvtContext, UserData, With};
use eyre::{OptionExt as _, Result, WrapErr as _, bail, ensure};
use poise::serenity_prelude::all::{
    ComponentInteraction, ComponentInteractionDataKind, CreateActionRow, ReactionType, Role, RoleId,
};
use poise::serenity_prelude::{CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption};
use poise::{CreateReply, serenity_prelude as serenity};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SHOW_ID: &str = "role_buttons.show";
pub const SELECT_ID: &str = "role_buttons.select";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigT {
    buttons: BTreeMap<String, RoleButtonData>,
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

pub async fn btn_show_role_selection(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    interaction: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let role_set_id = param;
    let user_id = interaction.user.id;
    let guild_id = interaction.guild_id.some()?;

    let role_button = read_role_button_data(ctx.user_data, role_set_id).await?;

    {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        let member = guild.members.get(&user_id).ok_or_eyre("No member")?;
        role_selection_message(role_set_id, &guild.roles, &member.roles.iter().collect(), role_button.roles)?
    }
    .respond_to_component(ctx.serenity_context, interaction)
    .await?;

    if let Some(on_click_role) = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        let member = guild.members.get(&user_id).ok_or_eyre("No member")?;
        role_button.on_click.filter(|role| !member.roles.contains(role))
    } {
        ctx.serenity_context.http.add_member_role(guild_id, user_id, on_click_role, None).await?;
    };

    Ok(())
}

pub async fn select_roles(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    interaction: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let role_set_id = param;
    let guild_id = interaction.guild_id.some()?;
    let ComponentInteractionDataKind::StringSelect { values } = interaction.data.kind.clone() else {
        bail!("Unexpected interaction kind: {:?}", interaction.data.kind);
    };

    let role_button = read_role_button_data(ctx.user_data, role_set_id).await?;
    let selectable: HashSet<_> = role_button.roles.iter().map(|r| r.role_id).collect();

    let selected: HashSet<_> = values.into_iter().filter_map(|s| s.parse().ok()).collect();
    let selected: HashSet<_> = selected.intersection(&selectable).collect();

    let member = guild_id.member(ctx.serenity_context, interaction.user.id).await.wrap_err("No member")?;
    let current: HashSet<_> = member.roles.iter().cloned().collect();
    let current: HashSet<_> = current.intersection(&selectable).collect();

    for &role_id in selected.difference(&current) {
        member.add_role(ctx.serenity_context, role_id).await?;
    }
    for &role_id in current.difference(&selected) {
        member.remove_role(ctx.serenity_context, role_id).await?;
    }

    {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        role_selection_message(role_set_id, &guild.roles, &selected, role_button.roles)?
    }
    .respond_to_component(ctx.serenity_context, interaction)
    .await?;

    Ok(())
}

async fn read_role_button_data(data: &impl With<ConfigT>, message_id: &str) -> Result<RoleButtonData> {
    let data = data.with(|cfg| Ok(cfg.buttons.get(message_id).ok_or_eyre("Unknown role button")?.clone())).await?;
    ensure!(!data.roles.is_empty(), "No roles have been configured for this button");
    Ok(data)
}

fn role_selection_message(
    role_set_id: &str,
    guild_roles: &HashMap<RoleId, Role>,
    member_roles: &HashSet<&RoleId>,
    selectable_roles: impl IntoIterator<Item = RoleData>,
) -> Result<CreateReply> {
    let options: Vec<_> = selectable_roles
        .into_iter()
        .filter_map(|role| {
            Some(
                CreateSelectMenuOption::new(
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

    Ok(CreateReply::new()
        .components(vec![CreateActionRow::SelectMenu(
            CreateSelectMenu::new(format!("{SELECT_ID}:{role_set_id}"), CreateSelectMenuKind::String { options })
                .min_values(0)
                .max_values(max_values),
        )])
        .ephemeral(true))
}
