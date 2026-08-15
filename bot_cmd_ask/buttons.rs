use crate::ask::{AskPlayer, AskPlayerState, AskRoleId};
use crate::schedule_updates::spawn_delayed_update;
use crate::{
    ConfigT, JOIN_ADVANCED_SUBMIT_BUTTON_ID, LEAVE_SERVER_BUTTON_ID, StateT, worker_ask_update, worker_game_roles,
};
use bot_core::ext::create_reply::CreateReplyExt;
use bot_core::ext::option::OptionExt;
use bot_core::ext::set::{BTreeSetExt, ToggleResult};
use bot_core::{EvtContext, State, With};
use chrono::TimeDelta;
use chrono::prelude::{DateTime, Utc};
use eyre::{Context, OptionExt as _, Result, bail};
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, Colour, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed, CreateInputText,
    CreateQuickModal, InputTextStyle, MessageId,
};
use std::collections::btree_map;
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
