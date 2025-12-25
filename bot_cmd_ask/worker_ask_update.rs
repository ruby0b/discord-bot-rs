use crate::ConfigT;
use crate::ask::Ask;
use bot_core::With;
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{Builder as _, Context, MessageId, colours};
use std::fmt::Debug;
use tokio::sync::mpsc;

#[derive(Debug)]
pub(crate) enum Command {
    Update(MessageId),
    Remove(MessageId),
}

pub(crate) async fn work(ctx: Context, data: impl With<ConfigT>, mut rx: mpsc::Receiver<Command>) {
    loop {
        if let Err(error) = {
            let Some(cmd) = rx.recv().await else { break };
            match cmd {
                Command::Update(message_id) => {
                    tracing::debug!("Updating ask {message_id}");
                    update_ask(&ctx, &data, message_id).await
                }
                Command::Remove(message_id) => {
                    tracing::debug!("Removing ask {message_id}");
                    remove_ask(&ctx, &data, message_id).await
                }
            }
        } {
            tracing::error!("Error in ask update worker: {error:?}");
        }
    }
}

async fn update_ask(ctx: &Context, data: &impl With<ConfigT>, msg_id: MessageId) -> Result<Ask> {
    let (ask, ping) = data
        .with_mut(|cfg| {
            let ask = cfg.asks.get_mut(&msg_id).ok_or_eyre("Can't update missing ask")?;
            Ok((ask.clone(), ask.ping(msg_id)))
        })
        .await?;

    ask.edit_message().execute(ctx, (ask.channel_id, msg_id, None)).await?;

    if let Some(ping) = ping {
        ping.execute(ctx, (ask.channel_id, None)).await?;
    };

    Ok(ask)
}

async fn remove_ask(ctx: &Context, data: &impl With<ConfigT>, msg_id: MessageId) -> Result<Ask> {
    let ask = data.with_mut(|cfg| Ok(cfg.asks.remove(&msg_id).ok_or_eyre("Can't remove missing ask")?.clone())).await?;

    ask.edit_message()
        .embed(ask.embed().colour(colours::branding::BLACK))
        .components(vec![])
        .execute(ctx, (ask.channel_id, msg_id, None))
        .await?;

    Ok(ask)
}
