use crate::worker_ask_update::Command;
use crate::{Ask, ConfigT, StateT};
use bot_core::{OptionExt as _, State, With};
use chrono::{TimeDelta, Utc};
use eyre::{OptionExt as _, Result};
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
        send(&data, Command::Update(msg_id)).await
    });

    let disable = (expiration + (ask.start_time - Utc::now())).to_std().unwrap_or_default();
    spawn(data.clone(), async move |data| {
        tokio::time::sleep(disable).await;
        send(&data, Command::Remove(msg_id)).await
    });

    if ask.thumbnail_url.is_none() {
        spawn(data.clone(), async move |data| {
            fetch_game_thumbnail(&data, msg_id).await?;
            send(&data, Command::Update(msg_id)).await
        });
    }

    if ask.description.is_none() {
        spawn(data.clone(), async move |data| {
            fetch_game_description(&data, msg_id).await?;
            send(&data, Command::Update(msg_id)).await
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

async fn send(data: &impl State<StateT>, cmd: Command) -> Result<()> {
    Ok(data.state().ask_update_sender.get().some()?.send(cmd).await?)
}

/// Search for a thumbnail for the ask message
async fn fetch_game_thumbnail(data: &(impl With<ConfigT> + State<StateT>), msg_id: MessageId) -> Result<()> {
    let thumbnail_url = {
        let Some(ask) = data.with_ok(|cfg| cfg.asks.get(&msg_id).cloned()).await? else {
            return Ok(());
        };
        if ask.thumbnail_url.is_some() {
            return Ok(());
        }

        let mut queries = vec![];
        if let Some(url) = ask.url {
            queries.push(format!("{} site:{}", ask.title, url));
        }
        queries.push(format!("{} Game", ask.title));

        let thumbnail_url = {
            let mut found_url: Option<String> = None;
            for query in &queries {
                if let Some(url) = search_image(query, data.state().serpapi_token.get().some()?).await? {
                    found_url = Some(url);
                    break;
                }
            }
            found_url.ok_or_eyre(format!("No images found for queries: {queries:?}"))?
        };

        thumbnail_url
    };

    data.with_mut_ok(|cfg| {
        let Some(ask) = cfg.asks.get_mut(&msg_id) else { return };
        ask.thumbnail_url = Some(thumbnail_url.clone());
    })
    .await
}

async fn search_image(query: &str, serpapi_token: &str) -> Result<Option<String>> {
    let result = reqwest::Client::new()
        .get("https://serpapi.com/search")
        .query(&[("api_key", serpapi_token), ("engine", "google_images"), ("hl", "en"), ("gl", "us"), ("q", query)])
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok((|| {
        Some(
            result
                .as_object()?
                .get("images_results")?
                .as_array()?
                .first()?
                .as_object()?
                .get("original")?
                .as_str()?
                .to_string(),
        )
    })())
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
        let mut description = match &ask.url {
            Some(url) => {
                tracing::debug!("fetching game description from {}", url);
                let html = reqwest::get(url.as_str()).await?.text().await?;
                let document = scraper::Html::parse_document(&html);
                let selector = scraper::Selector::parse(".game_description_snippet").unwrap();
                let element = document.select(&selector).next().ok_or_eyre(format!("No game description on {url}"))?;
                element.text().collect::<String>()
            }
            None => return Ok(()),
        };
        description.truncate(1024);
        description
    };

    data.with_mut_ok(|cfg| {
        let Some(ask) = cfg.asks.get_mut(&msg_id) else { return };
        ask.description = Some(description);
    })
    .await
}
