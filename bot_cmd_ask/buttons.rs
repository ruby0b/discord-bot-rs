use crate::update_worker::UpdateCommand;
use crate::{ConfigT, LEAVE_SERVER_BUTTON_ID, StateT};
use bot_core::{EvtContext, OptionExt, State, With};
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{
    ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton, CreateInputText,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateQuickModal, InputTextStyle,
};
use std::time::Duration;

pub enum AskButton {
    Join,
    Leave,
    Decline,
}

pub async fn button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
    ask_button: AskButton,
) -> Result<()> {
    let player_id = component.user.id;

    let response = ctx
        .user_data
        .with_mut(|cfg| {
            let ask = cfg.asks.get_mut(&component.message.id).ok_or_eyre("unknown ask")?;
            Ok(match ask_button {
                AskButton::Join => {
                    ask.declined_players.retain(|&x| x != player_id);
                    if !ask.full() && !ask.players.contains(&player_id) {
                        ask.players.push(player_id);
                    }
                    CreateInteractionResponse::Acknowledge
                }
                AskButton::Leave => {
                    if !ask.players.contains(&player_id)
                        && !ask.declined_players.contains(&player_id)
                    {
                        leave_server_response()
                    } else {
                        ask.players.retain(|&x| x != player_id);
                        ask.declined_players.retain(|&x| x != player_id);
                        CreateInteractionResponse::Acknowledge
                    }
                }
                AskButton::Decline => {
                    ask.players.retain(|&x| x != player_id);
                    if ask.declined_players.contains(&player_id) {
                        leave_server_response()
                    } else {
                        ask.declined_players.push(player_id);
                        CreateInteractionResponse::Acknowledge
                    }
                }
            })
        })
        .await?;

    component.create_response(ctx.serenity_context, response).await?;

    ctx.user_data
        .state()
        .update_sender
        .get()
        .some()?
        .send(UpdateCommand::Update(component.message.id))
        .await?;

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
