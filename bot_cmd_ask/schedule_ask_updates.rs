use crate::{Ask, ConfigT};
use bot_core::With;
use chrono::{TimeDelta, Utc};
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{self as serenity, Builder, Context, MessageId};

pub(crate) async fn schedule_ask_updates(
    ctx: &Context,
    data: &impl With<ConfigT>,
    ask: &Ask,
    msg_id: MessageId,
    expiration: TimeDelta,
) {
    let start = ask.start_time.signed_duration_since(Utc::now()).to_std().unwrap_or_default();
    spawn(ctx.clone(), data.clone(), async move |ctx, data| {
        tokio::time::sleep(start).await;
        update_ask_message(&ctx, &data, msg_id).await
    });

    let disable = (expiration + (ask.start_time - Utc::now())).to_std().unwrap_or_default();
    spawn(ctx.clone(), data.clone(), async move |ctx, data| {
        tokio::time::sleep(disable).await;
        disable_ask_message(&ctx, &data, msg_id).await
    });

    if ask.thumbnail_url.is_none() {
        spawn(ctx.clone(), data.clone(), async move |ctx, data| {
            fetch_game_thumbnail(&ctx, &data, msg_id).await
        });
    }

    if ask.description.is_none() {
        spawn(ctx.clone(), data.clone(), async move |ctx, data| {
            fetch_game_description(&ctx, &data, msg_id).await
        });
    }
}

fn spawn<F, R, D: Sync + Send + 'static>(
    ctx: Context,
    data: D,
    future: impl FnOnce(Context, D) -> F + Send + 'static,
) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = Result<R>> + Send + 'static,
    R: Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = future(ctx, data).await {
            tracing::error!("Error in task: {e:?}");
        };
    })
}

/// Search for a thumbnail for the ask message and update it
async fn fetch_game_thumbnail(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    let (channel_id, thumbnail_url) = {
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
        let url = image_search::urls(image_search::Arguments::new(&query, 1))
            .await
            .ok()
            .and_then(|x| x.first().cloned());
        (ask.channel_id, url)
    };

    let Some(edit) = data
        .with_mut_ok(|cfg| {
            let ask = cfg.asks.get_mut(&msg_id)?;
            ask.thumbnail_url = thumbnail_url;
            Some(ask.edit_message())
        })
        .await?
    else {
        return Ok(());
    };

    edit.execute(ctx, (channel_id, msg_id, None)).await?;

    Ok(())
}

/// Fetch a description for the game and update the ask message
async fn fetch_game_description(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    let (channel_id, description) = {
        let Some(ask) = data.with_ok(|cfg| cfg.asks.get(&msg_id).cloned()).await? else {
            return Ok(());
        };
        if ask.description.is_some() {
            return Ok(());
        }
        let description = match &ask.url {
            Some(url) => {
                tracing::debug!("fetching game description from {}", url);
                let html = reqwest::get(url.as_str()).await?.text().await?;
                let document = scraper::Html::parse_document(&html);
                let selector = scraper::Selector::parse(".game_description_snippet").unwrap();
                document.select(&selector).next().map(|x| x.text().collect::<String>())
            }
            None => return Ok(()),
        };
        (ask.channel_id, description)
    };

    let Some(edit) = data
        .with_mut_ok(|cfg| {
            let ask = cfg.asks.get_mut(&msg_id)?;
            ask.description = description;
            Some(ask.edit_message())
        })
        .await?
    else {
        return Ok(());
    };

    edit.execute(ctx, (channel_id, msg_id, None)).await?;

    Ok(())
}

async fn update_ask_message(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    tracing::info!("Updating ask {msg_id}");

    let (channel_id, edit, ping) = data
        .with_mut(|cfg| {
            let ask = cfg.asks.get_mut(&msg_id).ok_or_eyre("Can't update missing ask")?;
            Ok((ask.channel_id, ask.edit_message(), ask.ping(msg_id)))
        })
        .await?;

    edit.execute(ctx, (channel_id, msg_id, None)).await?;

    if let Some(ping) = ping {
        ping.execute(ctx, (channel_id, None)).await?;
    };

    Ok(())
}

async fn disable_ask_message(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    tracing::info!("Disabling ask {msg_id}");

    let (channel_id, edit, embed) = data
        .with_mut(|cfg| {
            let ask = cfg.asks.remove(&msg_id).ok_or_eyre("Can't remove missing ask")?;
            Ok((ask.channel_id, ask.edit_message(), ask.embed()))
        })
        .await?;

    edit.embed(embed.colour(serenity::colours::branding::BLACK))
        .components(vec![])
        .execute(ctx, (channel_id, msg_id, None))
        .await?;

    Ok(())
}
