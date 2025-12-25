use crate::ask::Ask;
use crate::schedule_updates::schedule_ask_updates;
use crate::{ConfigT, StateT};
use bot_core::{CmdContext, State, With, naive_time_to_next_datetime};
use chrono::{NaiveTime, Utc};
use eyre::Result;
use poise::serenity_prelude::{CreateAllowedMentions, RoleId};
use url::Url;

/// Find players to play a game with you
#[poise::command(slash_command)]
pub async fn ask<D: With<ConfigT> + State<StateT>>(
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
    let (game_with_name, expiration) = ctx
        .data()
        .with(|cfg| {
            let game = cfg
                .games
                .iter()
                .find(|(_, game)| game.title_pattern.0.is_match(&title).is_ok_and(|m| m))
                .map(|(name, game)| (name.clone(), game.clone()));
            Ok((game, cfg.expiration))
        })
        .await?;
    let game_name = game_with_name.as_ref().map(|(name, _)| name.clone());
    let game = game_with_name.as_ref().map(|(_, game)| game);

    let role_id = match role_from_game_name(&ctx, game_name.as_deref()) {
        Some(x) => Some(x),
        None => role_from_category_name(&ctx).await,
    };

    let defaults = game.map(|g| &g.defaults);
    let ask = Ask {
        players: vec![ctx.author().id],
        declined_players: vec![],
        min_players: min_players.or(defaults.as_ref().and_then(|d| d.min_players)),
        max_players: max_players.or(defaults.as_ref().and_then(|d| d.max_players)),
        title,
        url: url.or(defaults.as_ref().and_then(|d| d.url.clone())),
        description: description.or(defaults.as_ref().and_then(|d| d.description.clone())),
        thumbnail_url: defaults.as_ref().and_then(|d| d.thumbnail_url.clone()),
        channel_id: ctx.channel_id(),
        role_id,
        start_time: start_time.and_then(naive_time_to_next_datetime).map_or_else(Utc::now, |dt| dt.to_utc()),
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

    schedule_ask_updates(ctx.data(), &ask, msg_id, expiration).await;

    Ok(())
}

fn role_from_game_name(ctx: &CmdContext<'_, impl Send + Sync>, game: Option<&str>) -> Option<RoleId> {
    let guild = ctx.guild()?;
    let game_name = game?;
    let role = guild.role_by_name(game_name)?;
    Some(role.id)
}

async fn role_from_category_name(ctx: &CmdContext<'_, impl Send + Sync>) -> Option<RoleId> {
    let guild_channel = ctx.guild_channel().await?;
    let category_name = guild_channel.parent_id?.name(ctx).await.ok()?;
    let guild = ctx.guild()?;
    Some(guild.role_by_name(&category_name)?.id)
}
