use crate::ask::{AskPlayer, AskPlayerState, AskRoleId};
use crate::schedule_updates::spawn_delayed_update;
use crate::{
    ConfigT, Game, JOIN_ADVANCED_SUBMIT_BUTTON_ID, LEAVE_SERVER_BUTTON_ID, SHOW_GAME_ROLES_SELECT_ID,
    SUBMIT_GAME_ROLES_SELECT_ID, StateT, worker_ask_update, worker_game_roles,
};
use bot_core::ext::create_reply::CreateReplyExt;
use bot_core::ext::option::OptionExt;
use bot_core::ext::set::{BTreeSetExt, ToggleResult};
use bot_core::{EvtContext, State, With};
use chrono::TimeDelta;
use chrono::prelude::{DateTime, Utc};
use eyre::{Context, OptionExt as _, Result, bail, ensure};
use itertools::Itertools as _;
use poise::CreateReply;
use poise::serenity_prelude::prelude::Mentionable;
use poise::serenity_prelude::{
    ButtonStyle, Colour, ComponentInteraction, ComponentInteractionDataKind, CreateActionRow, CreateButton,
    CreateEmbed, CreateInputText, CreateQuickModal, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption,
    InputTextStyle, MessageId, RoleId,
};
use std::collections::{BTreeMap, HashSet, btree_map};
use std::time::Duration;

pub enum AskEvent {
    Join(DateTime<Utc>),
    Leave,
    Decline,
}

pub async fn btn_join(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    interaction: &ComponentInteraction,
) -> Result<()> {
    button_pressed(ctx, interaction, interaction.message.id, AskEvent::Join(Utc::now())).await
}

pub async fn btn_leave(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    interaction: &ComponentInteraction,
) -> Result<()> {
    button_pressed(ctx, interaction, interaction.message.id, AskEvent::Leave).await
}

pub async fn btn_decline(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    interaction: &ComponentInteraction,
) -> Result<()> {
    button_pressed(ctx, interaction, interaction.message.id, AskEvent::Decline).await
}

pub async fn btn_join_advanced(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    interaction: &ComponentInteraction,
) -> Result<()> {
    let ask_id = interaction.message.id;
    let action_row = CreateActionRow::Buttons(vec![
        CreateButton::new(format!("{JOIN_ADVANCED_SUBMIT_BUTTON_ID}:{ask_id}:10"))
            .label("+10 minutes")
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("{JOIN_ADVANCED_SUBMIT_BUTTON_ID}:{ask_id}:20"))
            .label("+20 minutes")
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("{JOIN_ADVANCED_SUBMIT_BUTTON_ID}:{ask_id}:30"))
            .label("+30 minutes")
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("{JOIN_ADVANCED_SUBMIT_BUTTON_ID}:{ask_id}:60"))
            .label("+1 hour")
            .style(ButtonStyle::Primary),
        CreateButton::new(format!("{JOIN_ADVANCED_SUBMIT_BUTTON_ID}:{ask_id}:120"))
            .label("+2 hours")
            .style(ButtonStyle::Primary),
    ]);
    CreateReply::new()
        .ephemeral(true)
        .components(vec![action_row])
        .respond_to_component(ctx.serenity_context, interaction)
        .await?;
    Ok(())
}

pub async fn btn_join_advanced_submit(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    interaction: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let (ask_id_param, offset_param) = param.split_once(':').ok_or_eyre("Invalid parameter format")?;
    let ask_id = ask_id_param.parse::<MessageId>().wrap_err("Invalid ask ID")?;
    let offset = offset_param.parse::<u8>().wrap_err("Invalid time offset")?;
    let now = Utc::now();
    let ask_start_time =
        ctx.user_data.with(|cfg| cfg.asks.get(&ask_id).map(|ask| ask.start_time).ok_or_eyre("Unknown /ask")).await?;
    let origin = if ask_start_time > now { ask_start_time } else { now };
    let entered_at = origin + chrono::Duration::minutes(offset as i64);
    button_pressed(ctx, interaction, ask_id, AskEvent::Join(entered_at)).await?;
    spawn_delayed_update(ctx.user_data, ask_id, (entered_at - Utc::now()).max(TimeDelta::zero()).to_std()?);
    Ok(())
}

pub async fn button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    interaction: &ComponentInteraction,
    ask_id: MessageId,
    event: AskEvent,
) -> Result<()> {
    interaction.defer(ctx.serenity_context).await?;

    let user_id = interaction.user.id;
    let reply = ctx
        .user_data
        .with_mut(|cfg| {
            let ask = cfg.asks.get_mut(&ask_id).ok_or_eyre("Unknown /ask")?;
            Ok(match event {
                AskEvent::Join(entered_at) => {
                    ask.players.insert(user_id, AskPlayer { entered_at, state: AskPlayerState::Joined });
                    None
                }
                AskEvent::Leave => {
                    if !ask.players.contains_key(&user_id) {
                        Some(leave_server_reply())
                    } else {
                        ask.players.retain(|&x, _| x != user_id);
                        None
                    }
                }
                AskEvent::Decline => match ask.players.entry(user_id) {
                    btree_map::Entry::Vacant(entry) => {
                        entry.insert(AskPlayer { entered_at: Utc::now(), state: AskPlayerState::Declined });
                        None
                    }
                    btree_map::Entry::Occupied(mut entry) => match entry.get().state {
                        AskPlayerState::Declined => Some(leave_server_reply()),
                        AskPlayerState::Joined => {
                            entry.insert(AskPlayer { entered_at: Utc::now(), state: AskPlayerState::Declined });
                            None
                        }
                    },
                },
            })
        })
        .await?;

    if let Some(reply) = reply {
        reply.followup_to_component(ctx.serenity_context, interaction).await?;
    }

    ctx.user_data.state().ask_update_sender.get().some()?.send(worker_ask_update::Command::Update(ask_id)).await?;

    Ok(())
}

fn leave_server_reply() -> CreateReply {
    CreateReply::new().ephemeral(true).content("Press again to leave the server").components(vec![
        CreateActionRow::Buttons(vec![
            CreateButton::new(LEAVE_SERVER_BUTTON_ID).label("Leave Server").style(ButtonStyle::Danger),
        ]),
    ])
}

pub async fn btn_leave_server(ctx: EvtContext<'_, impl With<ConfigT>>, component: &ComponentInteraction) -> Result<()> {
    CreateQuickModal::new("You have been banned!")
        .field(CreateInputText::new(InputTextStyle::Short, "Ban Reason", "").value("You pressed the button :("))
        .timeout(Duration::from_secs(2 * 60))
        .execute(ctx.serenity_context, component.id, &component.token)
        .await?;
    Ok(())
}

pub async fn btn_toggle_game_role(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
) -> Result<()> {
    component.defer(ctx.serenity_context).await?;

    let user_id = component.user.id;
    let response = ctx
        .user_data
        .with_mut(|cfg| {
            let ask = cfg.asks.get(&component.message.id).ok_or_eyre("Unknown /ask")?;
            let AskRoleId::KnownGame(game_role_id) = ask.role_id else {
                bail!("No game role is associated with this ask.")
            };
            let game = cfg.games.get_mut(&game_role_id).ok_or_eyre("Unexpected: The game no longer exists.")?;
            Ok(match game.opted_out_users.toggle(user_id) {
                ToggleResult::Inserted => format!("🔕 Unsubscribed from {game_role_id}"),
                ToggleResult::Removed => format!("🔔 Subscribed to {game_role_id}"),
            })
        })
        .await?;

    CreateReply::new()
        .embed(CreateEmbed::new().colour(Colour::GOLD).description(response))
        .ephemeral(true)
        .followup_to_component(ctx.serenity_context, component)
        .await?;

    ctx.user_data.state().game_role_sender.get().some()?.send(worker_game_roles::Command::Update).await?;

    Ok(())
}

pub async fn btn_show_parent_role_buttons(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    interaction: &ComponentInteraction,
) -> Result<()> {
    let user_id = interaction.user.id;
    let guild_id = interaction.guild_id.some()?;

    let parent_roles: HashSet<RoleId> =
        ctx.user_data.with_ok(|cfg| cfg.games.values().map(|game| game.parent_role).collect()).await?;
    ensure!(!parent_roles.is_empty(), "There are no game roles");
    ensure!(parent_roles.len() <= 25, "Discord can't display more than 25 options");

    let components = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        let member = guild.members.get(&user_id).ok_or_eyre("No member")?;
        let member_roles: HashSet<RoleId> = member.roles.iter().copied().collect();
        parent_roles
            .into_iter()
            .filter(|role_id| member_roles.contains(role_id))
            .filter_map(|role_id| guild.roles.get(&role_id))
            .sorted_by_key(|role| &role.name)
            .map(|role| {
                CreateButton::new(format!("{SHOW_GAME_ROLES_SELECT_ID}:{}", role.id))
                    .label(role.name.clone())
                    .style(ButtonStyle::Primary)
            })
            .chunks(5)
            .into_iter()
            .map(|row_buttons| CreateActionRow::Buttons(row_buttons.collect_vec()))
            .collect_vec()
    };

    CreateReply::new()
        .components(components)
        .ephemeral(true)
        .respond_to_component(ctx.serenity_context, interaction)
        .await?;

    Ok(())
}

pub async fn btn_show_game_role_selection(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    interaction: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let parent_role = param.parse::<RoleId>()?;
    let user_id = interaction.user.id;
    let guild_id = interaction.guild_id.some()?;

    let game_roles: BTreeMap<RoleId, Game> = ctx
        .user_data
        .with_ok(|cfg| {
            cfg.games.iter().filter(|(_, game)| game.parent_role == parent_role).map(|(k, v)| (*k, v.clone())).collect()
        })
        .await?;
    ensure!(!game_roles.is_empty(), "There are no game roles for {}", parent_role.mention());
    ensure!(game_roles.len() <= 25, "Discord can't display more than 25 options");

    let options = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        game_roles
            .into_iter()
            .filter_map(|(role_id, game)| Some((guild.roles.get(&role_id)?, game)))
            .map(|(role, game)| {
                CreateSelectMenuOption::new(role.name.clone(), role.id.to_string())
                    .default_selection(!game.opted_out_users.contains(&user_id))
            })
            .collect_vec()
    };

    let max_values = options.len() as u8;
    CreateReply::new()
        .components(vec![CreateActionRow::SelectMenu(
            CreateSelectMenu::new(
                format!("{SUBMIT_GAME_ROLES_SELECT_ID}:{parent_role}"),
                CreateSelectMenuKind::String { options },
            )
            .min_values(0)
            .max_values(max_values),
        )])
        .ephemeral(true)
        .respond_to_component(ctx.serenity_context, interaction)
        .await?;

    Ok(())
}

pub async fn select_roles(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    interaction: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let parent_role = param.parse::<RoleId>()?;
    let user_id = interaction.user.id;
    let ComponentInteractionDataKind::StringSelect { values } = interaction.data.kind.clone() else {
        bail!("Unexpected interaction kind: {:?}", interaction.data.kind);
    };
    let selected: HashSet<RoleId> = values.into_iter().filter_map(|s| s.parse().ok()).collect();

    interaction.defer(ctx.serenity_context).await?;

    ctx.user_data
        .with_mut_ok(|cfg| {
            for (role_id, game) in &mut cfg.games {
                if game.parent_role == parent_role {
                    if selected.contains(role_id) {
                        game.opted_out_users.remove(&user_id);
                    } else {
                        // This incorrectly also removes game roles
                        // that were created after we sent the dropdown.
                        // To fix that we'd have to parse the original dropdown
                        // or store the unselected options somehow which would be annoying.
                        game.opted_out_users.insert(user_id);
                    }
                }
            }
        })
        .await?;

    ctx.user_data.state().game_role_sender.get().some()?.send(worker_game_roles::Command::Update).await?;

    Ok(())
}
