use crate::{ConfigT, Currency};
use bot_core::{CmdContext, With};
use eyre::Result;
use itertools::Itertools;
use poise::CreateReply;
use poise::serenity_prelude::{Colour, CreateEmbed, Mentionable};
use std::cmp::Reverse;

/// Check the economy leaderboard
#[poise::command(slash_command, guild_only)]
pub async fn leaderboard<D: With<ConfigT>>(ctx: CmdContext<'_, D>) -> Result<()> {
    let cur = Currency::read(ctx.data()).await?;
    let mut accounts =
        ctx.data().with_ok(|cfg| cfg.account.iter().map(|(id, account)| (*id, account.clone())).collect_vec()).await?;

    accounts.sort_by_key(|(_, account)| Reverse(account.balance));

    let leaderboard = accounts
        .iter()
        .enumerate()
        .map(|(i, (id, u))| format!("{} {} {}", placement(i + 1), id.mention(), cur.fmt(u.balance)))
        .join("\n");

    let embed = CreateEmbed::new().title("Leaderboard").description(leaderboard).colour(Colour::DARK_GOLD);

    let reply = CreateReply::new().embed(embed);

    ctx.send(reply).await?;
    Ok(())
}

fn placement(n: usize) -> String {
    match n {
        1 => "🥇".to_string(),
        2 => "🥈".to_string(),
        3 => "🥉".to_string(),
        _ => format!("{n}."),
    }
}
