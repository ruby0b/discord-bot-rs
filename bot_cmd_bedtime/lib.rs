/*
TODO add/remove bedtimes (make sure to update interval_set)
/add time Option<date>
/add should display the bedtime with toggle buttons for each weekday
*/

use bot_core::interval_set::IntervalSet;
use bot_core::iso_weekday::IsoWeekday;
use bot_core::serde::LiteralRegex;
use bot_core::timer_queue::{TimerCommand, spawn_timer_queue};
use bot_core::{OptionExt, State, With};
use chrono::{DateTime, Datelike, Days, TimeDelta, Utc, Weekday};
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::all::{GuildId, UserId};
use poise::serenity_prelude::prelude::Context;
use poise::serenity_prelude::{Member, RoleId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock, mpsc};
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

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bedtime {
    first: DateTime<Utc>,
    repeat: BTreeSet<IsoWeekday>,
}

impl Bedtime {
    fn next(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if self.first > now {
            Some(self.first)
        } else {
            let today = now.date_naive().and_time(self.first.time()).and_utc();
            (0..=7).map(|offset| today + Days::new(offset)).find(|&bedtime| {
                bedtime > now && self.repeat.contains(&IsoWeekday(bedtime.weekday()))
            })
        }
    }
}

#[derive(Default)]
pub struct StateT {
    bedtime_intervals: RwLock<BTreeMap<UserId, IntervalSet<DateTime<Utc>>>>,
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
    {
        let mut intervals = state.bedtime_intervals.write().await;
        *intervals = get_bedtime_intervals_and_prune(&data).await?;
    }
    state.timer_sender.set(spawn_timer_queue(move |bedtime| {
        let data = data.clone();
        let ctx = ctx.clone();
        async move {
            let data = data.clone();
            start_or_stop_bedtime(ctx.clone(), &data, bedtime).await
        }
    }))?;
    Ok(())
}

async fn get_bedtime_intervals_and_prune(
    data: &(impl With<ConfigT> + State<StateT> + State<GuildId>),
) -> Result<BTreeMap<UserId, IntervalSet<DateTime<Utc>>>> {
    data.with_mut_ok(|c| {
        let now = Utc::now() + TimeDelta::seconds(30);
        c.users
            .iter_mut()
            .map(|(&user_id, bedtimes)| {
                bedtimes.retain(|x| x.next(now).is_some());
                let intervals = bedtimes
                    .iter()
                    .filter_map(|x| x.next(now))
                    .map(|x| x..(x + c.duration))
                    .collect::<IntervalSet<_>>();
                (user_id, intervals)
            })
            .collect::<BTreeMap<_, _>>()
    })
    .await
}

async fn start_or_stop_bedtime(
    ctx: Context,
    data: &(impl With<ConfigT> + State<StateT> + State<GuildId>),
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

    let state: Arc<StateT> = data.state();
    {
        let mut intervals = state.bedtime_intervals.write().await;
        *intervals = get_bedtime_intervals_and_prune(data).await?;
    }

    Ok(())
}

async fn stop_bedtime(
    ctx: Context,
    data: &(impl With<ConfigT> + State<StateT> + State<GuildId>),
    member: Member,
) -> Result<()> {
    let name = member.display_name();
    let state: Arc<StateT> = data.state();
    let cfg = data.with_ok(|cfg| cfg.clone()).await?;

    if let Some(bedtime_range) = {
        let interval_sets = state.bedtime_intervals.read().await;
        interval_sets.get(&member.user.id).and_then(|bedtimes| bedtimes.find(Utc::now()))
    } {
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
    data: &(impl With<ConfigT> + State<StateT> + State<GuildId>),
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
