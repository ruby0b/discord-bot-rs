use crate::update_worker::UpdateCommand;
use crate::{Ask, ConfigT, StateT};
use bot_core::{OptionExt as _, State, With};
use chrono::{TimeDelta, Utc};
use eyre::{Context, OptionExt as _, Result, ensure};
use poise::serenity_prelude::MessageId;
use reqwest::header::CONTENT_TYPE;

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

        let mut queries = vec![];
        if let Some(url) = ask.url {
            queries.push(format!("{} site:{}", ask.title, url));
            queries.push(format!("{} {}", ask.title, url));
        }
        queries.push(format!("{} Game", ask.title));

        let thumbnail_url = {
            let mut found_url: Option<String> = None;
            for query in &queries {
                if let Some(url) = search_image(query).await? {
                    found_url = Some(url);
                    break;
                }
            }
            found_url.ok_or_eyre(format!("No images found for queries: {queries:?}"))?
        };

        validate_image_url(&thumbnail_url).await.wrap_err("Invalid thumbnail URL")?;

        thumbnail_url
    };

    data.with_mut_ok(|cfg| {
        let Some(ask) = cfg.asks.get_mut(&msg_id) else { return };
        ask.thumbnail_url = Some(thumbnail_url.clone());
    })
    .await
}

async fn search_image(query: &str) -> Result<Option<String>> {
    let search_result = image_search::urls(image_search::Arguments::new(query, 1)).await?;
    let thumbnail_url = search_result.first().cloned();
    Ok(thumbnail_url)
}

async fn validate_image_url(thumbnail_url: &str) -> Result<()> {
    let response = reqwest::get(thumbnail_url).await?;
    let content_type =
        response.headers().get(CONTENT_TYPE).ok_or_eyre("No content type")?.to_str()?;
    ensure!(
        matches!(content_type, "image/jpeg" | "image/png" | "image/webp" | "image/gif"),
        "Not an image content type: {content_type}"
    );
    let size = response.content_length();
    ensure!(size.is_some_and(|l| l > 50), "Suspiciously small image of size {size:?}");
    Ok(())
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
                let element = document
                    .select(&selector)
                    .next()
                    .ok_or_eyre(format!("No game description on {url}"))?;
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
