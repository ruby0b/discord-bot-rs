use crate::update_worker::UpdateCommand;
use crate::{Ask, ConfigT, StateT};
use bot_core::{OptionExt, State, With};
use chrono::{TimeDelta, Utc};
use eyre::Result;
use poise::serenity_prelude::MessageId;

pub(crate) async fn schedule_ask_updates(
    data: &(impl With<ConfigT> + State<StateT>),
    ask: &Ask,
    msg_id: MessageId,
    expiration: TimeDelta,
) {
    let start = ask.start_time.signed_duration_since(Utc::now()).to_std().unwrap_or_default();
    spawn(data.clone(), async move |data| {
        tokio::time::sleep(start).await;
        send(&data, UpdateCommand::Update(msg_id)).await
    });

    let disable = (expiration + (ask.start_time - Utc::now())).to_std().unwrap_or_default();
    spawn(data.clone(), async move |data| {
        tokio::time::sleep(disable).await;
        send(&data, UpdateCommand::Remove(msg_id)).await
    });

    if ask.thumbnail_url.is_none() {
        spawn(data.clone(), async move |data| {
            fetch_game_thumbnail(&data, msg_id).await?;
            send(&data, UpdateCommand::Update(msg_id)).await
        });
    }

    if ask.description.is_none() {
        spawn(data.clone(), async move |data| {
            fetch_game_description(&data, msg_id).await?;
            send(&data, UpdateCommand::Update(msg_id)).await
        });
    }
}

fn spawn<F, R, D: Sync + Send + 'static>(
    data: D,
    future: impl FnOnce(D) -> F + Send + 'static,
) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = Result<R>> + Send + 'static,
    R: Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = future(data).await {
            tracing::error!("Error in task: {e:?}");
        };
    })
}

async fn send(data: &impl State<StateT>, cmd: UpdateCommand) -> Result<()> {
    Ok(data.state().update_sender.get().some()?.send(cmd).await?)
}

/// Search for a thumbnail for the ask message
async fn fetch_game_thumbnail(data: &impl With<ConfigT>, msg_id: MessageId) -> Result<()> {
    let thumbnail_url = {
        let Some(ask) = data.with_ok(|cfg| cfg.asks.get(&msg_id).cloned()).await? else {
            return Ok(());
        };
        if ask.thumbnail_url.is_some() {
            return Ok(());
        }
        let query = match ask.url {
            Some(url) => format!("{} site:{}", ask.title, url),
            None => format!("{} Game", ask.title),
        };
        image_search::urls(image_search::Arguments::new(&query, 1))
            .await
            .ok()
            .and_then(|x| x.first().cloned())
    };

    data.with_mut_ok(|cfg| {
        let Some(ask) = cfg.asks.get_mut(&msg_id) else { return };
        ask.thumbnail_url = thumbnail_url.clone();
    })
    .await
}

/// Fetch a description for the game
async fn fetch_game_description(data: &impl With<ConfigT>, msg_id: MessageId) -> Result<()> {
    let description = {
        let Some(ask) = data.with_ok(|cfg| cfg.asks.get(&msg_id).cloned()).await? else {
            return Ok(());
        };
        if ask.description.is_some() {
            return Ok(());
        }
        match &ask.url {
            Some(url) => {
                tracing::debug!("fetching game description from {}", url);
                let html = reqwest::get(url.as_str()).await?.text().await?;
                let document = scraper::Html::parse_document(&html);
                let selector = scraper::Selector::parse(".game_description_snippet").unwrap();
                document.select(&selector).next().map(|x| x.text().collect::<String>())
            }
            None => return Ok(()),
        }
    };

    data.with_mut_ok(|cfg| {
        let Some(ask) = cfg.asks.get_mut(&msg_id) else { return };
        ask.description = description;
    })
    .await
}
