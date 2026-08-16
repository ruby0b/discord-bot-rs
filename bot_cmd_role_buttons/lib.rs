use bot_core::ext::create_reply::CreateReplyExt;
use bot_core::ext::option::OptionExt as _;
use bot_core::{EvtContext, UserData, With};
use eyre::{OptionExt as _, Result, WrapErr as _, ensure};
use poise::serenity_prelude::all::{
    ComponentInteraction, ComponentInteractionDataKind, CreateActionRow, ReactionType, Role, RoleId,
};
use poise::serenity_prelude::{CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption};
use poise::{CreateReply, serenity_prelude as serenity};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const SHOW_ROLE_SELECTION_ID: &str = "show_role_selection";

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
    let guild_id = interaction.guild_id.some()?;
    let guild_roles = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        guild.roles.clone()
    };

    let initial_response = {
        let role_button = read_role_button_data(ctx.user_data, role_set_id).await?;
        let member = {
            let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
            guild.members.get(&interaction.user.id).ok_or_eyre("No member")?.clone()
        };

        ensure!(!role_button.roles.is_empty(), "No roles have been configured for this button");

        role_selection_message(&guild_roles, &member.roles.iter().collect(), role_button.roles)?
            .respond_to_component(ctx.serenity_context, interaction)
            .await?;

        let initial_response = interaction.get_response(ctx.serenity_context).await?;

        if let Some(on_click_role) = role_button.on_click
            && !member.roles.contains(&on_click_role)
        {
            member.add_role(ctx.serenity_context, on_click_role).await?;
        };

        initial_response
    };

    while let Some(int) = initial_response.await_component_interaction(ctx.serenity_context).await {
        let ComponentInteractionDataKind::StringSelect { values } = int.data.kind.clone() else {
            tracing::error!("Unexpected interaction kind: {:?}", int.data.kind);
            continue;
        };

        let role_button = read_role_button_data(ctx.user_data, role_set_id).await?;
        let selectable: HashSet<_> = role_button.roles.iter().map(|r| r.role_id).collect();

        let selected: HashSet<_> = values.into_iter().filter_map(|s| s.parse().ok()).collect();
        let selected: HashSet<_> = selected.intersection(&selectable).collect();

        let member = guild_id.member(ctx.serenity_context, int.user.id).await.wrap_err("No member")?;
        let current: HashSet<_> = member.roles.iter().cloned().collect();
        let current: HashSet<_> = current.intersection(&selectable).collect();

        for &role_id in selected.difference(&current) {
            member.add_role(ctx.serenity_context, role_id).await?;
        }
        for &role_id in current.difference(&selected) {
            member.remove_role(ctx.serenity_context, role_id).await?;
        }

        role_selection_message(&guild_roles, &member.roles.iter().collect(), role_button.roles)?
            .update_to_component(ctx.serenity_context, interaction)
            .await?;
    }

    Ok(())
}

async fn read_role_button_data(data: &impl With<ConfigT>, message_id: &str) -> Result<RoleButtonData> {
    data.with(|cfg| Ok(cfg.buttons.get(message_id).ok_or_eyre("Unknown role button")?.clone())).await
}

fn role_selection_message(
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
            CreateSelectMenu::new("~roles", CreateSelectMenuKind::String { options })
                .min_values(0)
                .max_values(max_values),
        )])
        .ephemeral(true))
}
