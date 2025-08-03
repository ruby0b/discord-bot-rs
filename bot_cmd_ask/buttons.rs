use crate::ask::Ask;
use crate::{ConfigT, LEAVE_SERVER_BUTTON_ID};
use bot_core::{EvtContext, With};
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{
    Builder, ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton, CreateInputText,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateQuickModal, InputTextStyle,
};
use std::time::Duration;

pub enum AskButton {
    Join,
    Leave,
    Decline,
}

pub async fn button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
    ask_button: AskButton,
) -> Result<()> {
    let player_id = component.user.id;

    let success_response = |ask: &Ask| {
        CreateInteractionResponse::UpdateMessage(
            CreateInteractionResponseMessage::new()
                .embed(ask.embed())
                .components(vec![ask.action_row()]),
        )
    };

    let (channel_id, response, ping) = ctx
        .user_data
        .with_mut(|cfg| {
            let ask = cfg.asks.get_mut(&component.message.id).ok_or_eyre("unknown ask")?;
            let response = match ask_button {
                AskButton::Join => {
                    ask.declined_players.retain(|&x| x != player_id);
                    if ask.full() {
                        CreateInteractionResponse::Message(
                            CreateInteractionResponseMessage::new()
                                .ephemeral(true)
                                .content("Sorry, the lobby is already full"),
                        )
                    } else if ask.players.contains(&player_id) {
                        CreateInteractionResponse::Acknowledge
                    } else {
                        ask.players.push(player_id);
                        success_response(ask)
                    }
                }
                AskButton::Leave => {
                    if !ask.players.contains(&player_id)
                        && !ask.declined_players.contains(&player_id)
                    {
                        leave_server_response()
                    } else {
                        ask.players.retain(|&x| x != player_id);
                        ask.declined_players.retain(|&x| x != player_id);
                        success_response(ask)
                    }
                }
                AskButton::Decline => {
                    ask.players.retain(|&x| x != player_id);
                    if ask.declined_players.contains(&player_id) {
                        leave_server_response()
                    } else {
                        ask.declined_players.push(player_id);
                        success_response(ask)
                    }
                }
            };
            Ok((ask.channel_id, response, ask.ping(component.message.id)))
        })
        .await?;

    component.create_response(ctx.serenity_context, response).await?;

    if let Some(ping) = ping {
        ping.execute(ctx.serenity_context, (channel_id, None)).await?;
    }

    Ok(())
}

fn leave_server_response() -> CreateInteractionResponse {
    CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .ephemeral(true)
            .content("Press again to leave the server")
            .components(vec![CreateActionRow::Buttons(vec![
                CreateButton::new(LEAVE_SERVER_BUTTON_ID)
                    .label("Leave Server")
                    .style(ButtonStyle::Danger),
            ])]),
    )
}

pub async fn leave_server(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
) -> Result<()> {
    CreateQuickModal::new("You have been banned!")
        .field(
            CreateInputText::new(InputTextStyle::Short, "Ban Reason", "")
                .value("You pressed the button :("),
        )
        .timeout(Duration::from_secs(2 * 60))
        .execute(ctx.serenity_context, component.id, &component.token)
        .await?;
    Ok(())
}
