use crate::{ConfigT, LEAVE_SERVER_BUTTON_ID, StateT, worker_ask_update, worker_game_roles};
use bot_core::set_ext::{BTreeSetExt, ToggleResult};
use bot_core::{CreateReplyExt, EvtContext, OptionExt, State, With};
use eyre::{OptionExt as _, Result};
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, Colour, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed, CreateInputText,
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
                    if !ask.players.contains(&player_id) && !ask.declined_players.contains(&player_id) {
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
        .ask_update_sender
        .get()
        .some()?
        .send(worker_ask_update::Command::Update(component.message.id))
        .await?;

    Ok(())
}

fn leave_server_response() -> CreateInteractionResponse {
    CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new().ephemeral(true).content("Press again to leave the server").components(
            vec![CreateActionRow::Buttons(vec![
                CreateButton::new(LEAVE_SERVER_BUTTON_ID).label("Leave Server").style(ButtonStyle::Danger),
            ])],
        ),
    )
}

pub async fn leave_server(ctx: EvtContext<'_, impl With<ConfigT>>, component: &ComponentInteraction) -> Result<()> {
    CreateQuickModal::new("You have been banned!")
        .field(CreateInputText::new(InputTextStyle::Short, "Ban Reason", "").value("You pressed the button :("))
        .timeout(Duration::from_secs(2 * 60))
        .execute(ctx.serenity_context, component.id, &component.token)
        .await?;
    Ok(())
}

pub async fn toggle_game_role(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
) -> Result<()> {
    let user_id = component.user.id;
    let response = ctx
        .user_data
        .with_mut(|cfg| {
            let ask = cfg.asks.get(&component.message.id).ok_or_eyre("Unknown ask")?;
            let game_name = ask.known_game.as_deref().ok_or_eyre("No game role is associated with this ask.")?;
            let game = cfg.games.get_mut(game_name).ok_or_eyre("Unexpected: The game no longer exists.")?;
            Ok(match game.opted_out_users.toggle(user_id) {
                ToggleResult::Inserted => format!("🔕 Unsubscribed from {game_name}"),
                ToggleResult::Removed => format!("🔔 Subscribed to {game_name}"),
            })
        })
        .await?;

    CreateReply::new()
        .embed(CreateEmbed::new().colour(Colour::GOLD).description(response))
        .ephemeral(true)
        .respond_to_interaction(ctx.serenity_context, component)
        .await?;

    ctx.user_data.state().game_role_sender.get().some()?.send(worker_game_roles::Command::Update).await?;

    Ok(())
}
