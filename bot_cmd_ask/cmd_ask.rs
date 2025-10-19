use crate::ConfigT;
use crate::ask::Ask;
use crate::schedule_ask_updates::schedule_ask_updates;
use bot_core::{CmdContext, With, naive_time_to_next_datetime};
use chrono::{NaiveTime, Utc};
use eyre::Result;
use poise::serenity_prelude::CreateAllowedMentions;
use url::Url;

/// Find players to play a game with you
#[poise::command(slash_command)]
pub async fn ask<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Game title"] title: String,
    #[description = "Minimum number of players"] min_players: Option<u32>,
    #[description = "Maximum number of players"] max_players: Option<u32>,
    #[description = "Start time"]
    #[autocomplete = bot_core::autocomplete::time]
    start_time: Option<NaiveTime>,
    #[description = "Link to the game"] url: Option<Url>,
    #[description = "Game description"] description: Option<String>,
) -> Result<()> {
    let (default, expiration) = ctx
        .data()
        .with(|cfg| {
            let mut default = None;
            for (k, v) in cfg.defaults.iter() {
                if k.0.is_match(&title)? {
                    default = Some(v.clone());
                    break;
                }
            }
            Ok((default, cfg.expiration))
        })
        .await?;
    let default = default.as_ref();

    let role_id = async {
        let category_name = ctx.guild_channel().await?.parent_id?.name(ctx).await.ok()?;
        let guild = ctx.guild()?;
        Some(guild.role_by_name(&category_name)?.id)
    }
    .await;

    let ask = Ask {
        players: vec![ctx.author().id],
        declined_players: vec![],
        min_players: min_players.or(default.and_then(|d| d.min_players)),
        max_players: max_players.or(default.and_then(|d| d.max_players)),
        title,
        url: url.or(default.and_then(|d| d.url.clone())),
        description: description.or(default.and_then(|d| d.description.clone())),
        thumbnail_url: default.and_then(|d| d.thumbnail_url.clone()),
        channel_id: ctx.channel_id(),
        role_id,
        start_time: start_time
            .and_then(naive_time_to_next_datetime)
            .map_or_else(Utc::now, |dt| dt.to_utc()),
        pinged: false,
    };

    let msg_id = {
        let reply = poise::CreateReply::default()
            .content(format!("{} {}", ask.title, ask.content()))
            .embed(ask.embed())
            .allowed_mentions(CreateAllowedMentions::new().roles(ask.role_id))
            .components(vec![ask.action_row()]);
        let relpy_handle = ctx.send(reply).await?;
        relpy_handle.message().await?.id
    };

    ctx.data().with_mut_ok(|cfg| cfg.asks.insert(msg_id, ask.clone())).await?;

    schedule_ask_updates(ctx.serenity_context(), ctx.data(), &ask, msg_id, expiration).await;

    Ok(())
}
