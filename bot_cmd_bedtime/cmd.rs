use super::ConfigT;
use crate::bedtime::Bedtime;
use bot_core::{CmdContext, With, naive_time_to_next_datetime};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Utc};
use eyre::{OptionExt as _, Result};
use uuid::Uuid;

/// Set a bedtime
#[poise::command(slash_command, guild_only)]
pub async fn bedtime<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Time"]
    #[autocomplete = bot_core::autocomplete::time]
    time: NaiveTime,
    #[description = "Date"]
    // todo autocomplete
    date: Option<NaiveDate>,
) -> Result<()> {
    let bedtime = Bedtime {
        user: ctx.author().id,
        first: match date {
            Some(d) => NaiveDateTime::new(d, time).and_utc(),
            None => naive_time_to_next_datetime(time).ok_or_eyre("Gap in time")?.to_utc(),
        },
        repeat: Default::default(),
    };

    let id = ctx
        .data()
        .with_mut_ok(|cfg| {
            let id = Uuid::new_v4();
            cfg.bedtimes.insert(id, bedtime.clone());
            id
        })
        .await?;

    ctx.send(bedtime.reply(id, ctx.data(), Utc::now()).await?).await?;

    Ok(())
}

/// View your bedtimes
#[poise::command(slash_command, guild_only)]
pub async fn bedtimes<D: With<ConfigT>>(ctx: CmdContext<'_, D>) -> Result<()> {
    let now = Utc::now();
    let (next_id, next_bedtime) = ctx
        .data()
        .with_ok(|cfg| {
            cfg.bedtimes
                .iter()
                .filter(|(_, bedtime)| bedtime.user == ctx.author().id)
                .min_by_key(|(_, bedtime)| bedtime.next(now))
                .map(|(id, bedtime)| (*id, bedtime.clone()))
        })
        .await?
        .ok_or_eyre("You have no bedtimes.")?;

    ctx.send(next_bedtime.reply(next_id, ctx.data(), now).await?).await?;

    Ok(())
}
