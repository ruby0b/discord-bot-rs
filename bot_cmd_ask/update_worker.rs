use crate::ConfigT;
use crate::ask::Ask;
use bot_core::With;
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{Builder as _, Context, MessageId, colours};
use std::collections::HashMap;
use std::fmt::Debug;
use tokio::sync::mpsc;

#[derive(Debug)]
pub(crate) enum UpdateCommand {
    Update(MessageId),
    Remove(MessageId),
    Shutdown,
}

pub(crate) async fn ask_update_worker(
    ctx: Context,
    data: impl With<ConfigT>,
    mut rx: mpsc::Receiver<UpdateCommand>,
) {
    let mut valid_ask_cache = HashMap::<MessageId, Ask>::new();
    loop {
        let (message_id, result) = {
            let Some(cmd) = rx.recv().await else { break };
            match cmd {
                UpdateCommand::Update(message_id) => {
                    tracing::debug!("Updating ask {message_id}");
                    (message_id, update_ask(&ctx, &data, message_id).await)
                }
                UpdateCommand::Remove(message_id) => {
                    tracing::debug!("Removing ask {message_id}");
                    (message_id, remove_ask(&ctx, &data, message_id).await)
                }
                UpdateCommand::Shutdown => {
                    tracing::debug!("Shutting down ask update worker");
                    break;
                }
            }
        };

        match result {
            Ok(ask) => {
                valid_ask_cache.insert(message_id, ask);
            }
            Err(error) => {
                tracing::error!("Error in ask update worker: {error:?}");
                if let Some(ask) = valid_ask_cache.get(&message_id).cloned() {
                    tracing::error!("Restoring old ask data: {ask:?}");
                    if let Err(e) = data.with_mut_ok(|cfg| cfg.asks.insert(message_id, ask)).await {
                        tracing::error!("Error while restoring old ask data: {e:?}");
                    }
                }
            }
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
    let ask = data
        .with_mut(|cfg| {
            Ok(cfg.asks.remove(&msg_id).ok_or_eyre("Can't remove missing ask")?.clone())
        })
        .await?;

    ask.edit_message()
        .embed(ask.embed().colour(colours::branding::BLACK))
        .components(vec![])
        .execute(ctx, (ask.channel_id, msg_id, None))
        .await?;

    Ok(ask)
}
