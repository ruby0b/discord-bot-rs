use crate::ask::{Ask, AskPlayer, AskPlayerState, AskRoleId};
use crate::schedule_updates::schedule_ask_updates;
use crate::{ConfigT, StateT};
use bot_core::ext::option::OptionExt as _;
use bot_core::{CmdContext, State, With, naive_time_to_next_datetime};
use chrono::{NaiveTime, Utc};
use eyre::Result;
use poise::serenity_prelude::{CreateAllowedMentions, Guild, GuildChannel, RoleId};
use url::Url;

/// Find players to play a game with you
#[poise::command(slash_command)]
pub async fn ask<D: With<ConfigT> + State<StateT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Game title"] title: String,
    #[description = "Minimum number of players"] min_players: Option<u32>,
    #[description = "Maximum number of players"] max_players: Option<u32>,
    #[string]
    #[autocomplete = bot_core::autocomplete::time]
    #[description = "Start time"]
    start_time: Option<NaiveTime>,
    #[string]
    #[description = "Link to the game"]
    url: Option<Url>,
    #[description = "Game description"] description: Option<String>,
) -> Result<()> {
    let (game_with_name, expiration) = ctx
        .data()
        .with(|cfg| {
            let game = cfg
                .games
                .iter()
                .find(|(_, game)| game.title_pattern.0.is_match(&title).is_ok_and(|m| m))
                .map(|(&role_id, game)| (role_id, game.clone()));
            Ok((game, cfg.expiration))
        })
        .await?;
    let game_role_id = game_with_name.as_ref().map(|(role_id, _)| *role_id);
    let game = game_with_name.map(|(_, game)| game);

    let existing_game_role_id = {
        let guild = ctx.guild().some()?;
        if game_role_id.is_some_and(|id| guild.roles.contains_key(&id)) { game_role_id } else { None }
    };

    let role_id = if let Some(game_role_id) = existing_game_role_id {
        AskRoleId::KnownGame(game_role_id)
    } else {
        match role_from_channel_or_category_name(&ctx).await {
            Some(role_id) => AskRoleId::Other(role_id),
            None => AskRoleId::None,
        }
    };

    let now = Utc::now();
    let author_player = AskPlayer { state: AskPlayerState::Joined, entered_at: now };
    let defaults = game.map(|g| g.defaults);
    let ask = Ask {
        players: [(ctx.author().id, author_player)].into_iter().collect(),
        min_players: min_players.or(defaults.as_ref().and_then(|d| d.min_players)),
        max_players: max_players.or(defaults.as_ref().and_then(|d| d.max_players)),
        title,
        url: url.or(defaults.as_ref().and_then(|d| d.url.clone())),
        description: description.or(defaults.as_ref().and_then(|d| d.description.clone())),
        thumbnail_url: defaults.as_ref().and_then(|d| d.thumbnail_url.clone()),
        channel_id: ctx.channel_id(),
        role_id,
        start_time: start_time.and_then(naive_time_to_next_datetime).map_or(now, |dt| dt.to_utc()),
        pinged: false,
    };

    let msg_id = {
        let reply = poise::CreateReply::default()
            .content(format!("{} {}", ask.title, ask.content()))
            .embed(ask.embed())
            .allowed_mentions(CreateAllowedMentions::new().roles(ask.role_id.into_option()))
            .components(vec![ask.action_row()]);
        let reply_handle = ctx.send(reply).await?;
        reply_handle.message().await?.id
    };

    ctx.data().with_mut_ok(|cfg| cfg.asks.insert(msg_id, ask.clone())).await?;

    schedule_ask_updates(ctx.data(), &ask, msg_id, expiration).await;

    Ok(())
}

async fn role_from_channel_or_category_name(ctx: &CmdContext<'_, impl Send + Sync>) -> Option<RoleId> {
    let channel = ctx.guild_channel().await?;
    let guild = ctx.guild()?;
    role_from_channel_name(&guild, &channel).or_else(|| role_from_category_name(&guild, &channel))
}

fn role_from_channel_name(guild: &Guild, channel: &GuildChannel) -> Option<RoleId> {
    let channel_name = channel.name.to_lowercase().replace(['-', '_'], " ");
    let role = guild.roles.values().find(|role| role.name.to_lowercase() == channel_name)?;
    Some(role.id)
}

fn role_from_category_name(guild: &Guild, channel: &GuildChannel) -> Option<RoleId> {
    let category = guild.channels.get(&channel.parent_id?)?;
    let role = guild.role_by_name(&category.name)?;
    Some(role.id)
}
