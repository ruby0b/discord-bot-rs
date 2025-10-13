/*
TODO add/remove bedtimes (make sure to update interval_set)
TODO weekly bedtimes support
*/

#![feature(trait_alias)]

mod interval_set;
mod weekly_time;

use crate::interval_set::IntervalSet;
use crate::weekly_time::WeeklyTime;
use bot_core::serde::LiteralRegex;
use bot_core::timer_queue::{TimerCommand, spawn_timer_queue};
use bot_core::{OptionExt, State, With};
use chrono::{DateTime, TimeDelta, Utc};
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::all::{GuildId, UserId};
use poise::serenity_prelude::prelude::Context;
use poise::serenity_prelude::{Member, RoleId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::{OnceCell, mpsc};
use tokio::time::Instant;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    #[serde(with = "bot_core::serde::td_seconds")]
    #[default(TimeDelta::hours(6))]
    duration: TimeDelta,
    ignored_vc_description: Option<LiteralRegex>,
    role: Option<RoleId>,
    users: BTreeMap<UserId, BTreeSet<Bedtime>>,
}

#[derive(
    serde::Serialize, serde::Deserialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord,
)]
enum Bedtime {
    Weekly(WeeklyTime),
    Oneshot(DateTime<Utc>),
}

impl Bedtime {
    fn current(self) -> DateTime<Utc> {
        match self {
            Self::Weekly(_) => todo!("Weekly not yet supported"),
            Self::Oneshot(x) => x,
        }
    }
}

#[derive(Default)]
pub struct StateT {
    bedtime_intervals: OnceCell<BTreeMap<UserId, IntervalSet<DateTime<Utc>>>>,
    timer_sender: OnceCell<mpsc::Sender<TimerCommand<(UserId, StartOrStopBedtime)>>>,
}

#[derive(Debug)]
enum StartOrStopBedtime {
    Start,
    Stop,
}

pub async fn bedtime_loop(
    ctx: Context,
    data: impl With<ConfigT> + State<StateT> + State<GuildId>,
) -> Result<()> {
    let state: Arc<StateT> = data.state();

    state.bedtime_intervals.set(
        data.with_ok(|c| {
            c.users
                .iter()
                .map(|(&user_id, bedtimes)| (user_id, build_interval_set(bedtimes, c.duration)))
                .collect::<BTreeMap<_, _>>()
        })
        .await?,
    )?;

    state.timer_sender.set(spawn_timer_queue(move |bedtime| {
        start_or_stop_bedtime(ctx.clone(), data.clone(), bedtime)
    }))?;

    Ok(())
}

fn build_interval_set(
    bedtimes: &BTreeSet<Bedtime>,
    duration: TimeDelta,
) -> IntervalSet<DateTime<Utc>> {
    bedtimes
        .iter()
        .map(|x| {
            let time = x.current();
            time..(time + duration)
        })
        .collect()
}

async fn start_or_stop_bedtime(
    ctx: Context,
    data: impl With<ConfigT> + State<StateT> + State<GuildId>,
    (user_id, sleep_or_wake): (UserId, StartOrStopBedtime),
) -> Result<()> {
    let guild_id: GuildId = *data.state();
    let member = {
        let guild = ctx.cache.guild(guild_id).some()?;
        guild.members.get(&user_id).ok_or_eyre("guild member not found")?.clone()
    };

    match sleep_or_wake {
        StartOrStopBedtime::Start => start_bedtime(ctx, data, member).await?,
        StartOrStopBedtime::Stop => stop_bedtime(ctx, data, member).await?,
    }

    Ok(())
}

async fn stop_bedtime(
    ctx: Context,
    data: impl With<ConfigT> + State<StateT> + State<GuildId>,
    member: Member,
) -> Result<()> {
    let name = member.display_name();
    let state: Arc<StateT> = data.state();
    let cfg = data.with_ok(|cfg| cfg.clone()).await?;

    if let Some(bedtime_range) = state
        .bedtime_intervals
        .get()
        .some()?
        .get(&member.user.id)
        .and_then(|bedtimes| bedtimes.find(Utc::now()))
    {
        let end = bedtime_range.end;
        tracing::info!("🌙 {name} should still be sleeping until {end}, waiting until then");
        let timer = end.signed_duration_since(Utc::now());
        send_timer(&state, member.user.id, StartOrStopBedtime::Stop, timer).await?;
        return Ok(());
    };

    let Some(bedtime_role_id) = cfg.role else {
        tracing::warn!(
            "🌙 Bedtime role has been removed from config before I could take it away from {name}"
        );
        return Ok(());
    };

    if member.roles.contains(&bedtime_role_id) {
        tracing::info!("🌙 Removing bedtime role from {name}");
        member.remove_role(&ctx, bedtime_role_id).await?;
    } else {
        tracing::info!("🌙 {name} does not have the bedtime role, can't remove it");
    }

    Ok(())
}

async fn start_bedtime(
    ctx: Context,
    data: impl With<ConfigT> + State<StateT> + State<GuildId>,
    member: Member,
) -> Result<()> {
    let name = member.display_name();

    let cfg = data.with_ok(|cfg| cfg.clone()).await?;
    if let Some(bedtime_role_id) = cfg.role {
        if member.roles.contains(&bedtime_role_id) {
            tracing::info!("🌙 {name} already has the bedtime role");
        } else {
            tracing::info!("🌙 Giving {name} the bedtime role");
            member.add_role(&ctx, bedtime_role_id).await?;
            send_timer(&data.state(), member.user.id, StartOrStopBedtime::Stop, cfg.duration)
                .await?;
        }
    };

    if let Some(_channel_id) = {
        let guild = ctx.cache.guild(member.guild_id).some()?;
        guild.voice_states.get(&member.user.id).and_then(|x| x.channel_id)
    } {
        tracing::info!("🌙 Disconnecting {name}");
        member.guild_id.disconnect_member(&ctx, member.user.id).await?;
    }

    Ok(())
}

async fn send_timer(
    state: &StateT,
    user_id: UserId,
    sleep_or_wake: StartOrStopBedtime,
    timer: TimeDelta,
) -> Result<()> {
    state
        .timer_sender
        .get()
        .some()?
        .send(TimerCommand::AddTimer {
            data: (user_id, sleep_or_wake),
            when: Instant::now() + (timer + TimeDelta::seconds(1)).to_std()?,
        })
        .await?;
    Ok(())
}
