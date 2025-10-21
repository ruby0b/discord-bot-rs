mod bedtime;
mod buttons;

use crate::bedtime::Bedtime;
pub use crate::buttons::*;
use bot_core::interval_set::IntervalSet;
use bot_core::serde::LiteralRegex;
use bot_core::{CmdContext, OptionExt as _, State, With, get_member, naive_time_to_next_datetime};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, Utc};
use eyre::{OptionExt as _, Result};
use itertools::Itertools;
use poise::CreateReply;
use poise::serenity_prelude::all::{GuildId, UserId};
use poise::serenity_prelude::prelude::Context;
use poise::serenity_prelude::{Member, RoleId};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const TOGGLE_WEEKDAY_BUTTON_ID: &str = "bedtime.weekday";
pub const DELETE_BUTTON_ID: &str = "bedtime.delete";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    #[serde(with = "bot_core::serde::td_seconds")]
    #[default(TimeDelta::hours(6))]
    duration: TimeDelta,
    ignored_vc_description: Option<LiteralRegex>,
    role: Option<RoleId>,
    bedtimes: BTreeMap<Uuid, Bedtime>,
}

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

    ctx.send(CreateReply::new().embed(bedtime.embed()).components(bedtime.components(id))).await?;

    Ok(())
}

pub async fn bedtime_loop(ctx: Context, data: impl With<ConfigT> + State<GuildId>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_mins(1));
    interval.tick().await;
    loop {
        if let Err(error) = enforce_and_lift_bedtimes(&ctx, &data).await {
            tracing::error!("Error in bedtime loop: {error:?}");
        }
        interval.tick().await;
    }
}

async fn enforce_and_lift_bedtimes(
    ctx: &Context,
    data: &(impl With<ConfigT> + State<GuildId>),
) -> Result<()> {
    let guild_id: GuildId = *data.state();

    let now = Utc::now();
    let intervals_by_user = get_bedtime_intervals_and_prune(data, now).await?;
    let cfg = data.with_ok(|cfg| cfg.clone()).await?;

    for (&user_id, intervals) in intervals_by_user.iter() {
        let Some(member) = get_member(ctx, guild_id, user_id) else { continue };
        if intervals.find(now).is_some() {
            enforce_bedtime(ctx, &cfg, member).await?;
        } else {
            lift_bedtime(ctx, &cfg, member).await?;
        }
    }

    Ok(())
}

async fn get_bedtime_intervals_and_prune(
    data: &impl With<ConfigT>,
    now: DateTime<Utc>,
) -> Result<BTreeMap<UserId, IntervalSet<DateTime<Utc>>>> {
    data.with_mut_ok(|cfg| {
        let interval_sets = cfg
            .bedtimes
            .values()
            .chunk_by(|bedtime| bedtime.user)
            .into_iter()
            .map(|(user_id, bedtimes)| {
                let intervals = bedtimes
                    .into_iter()
                    .flat_map(|x| x.currently_relevant_bedtimes(now))
                    .map(|x| x..(x + cfg.duration))
                    .collect::<IntervalSet<_>>();
                (user_id, intervals)
            })
            .collect::<BTreeMap<_, _>>();

        // prune outdated bedtimes that don't repeat
        cfg.bedtimes.retain(|_, b| b.first >= (now - cfg.duration) || !b.repeat.is_empty());

        interval_sets
    })
    .await
}

async fn enforce_bedtime(ctx: &Context, cfg: &ConfigT, member: Member) -> Result<()> {
    let name = member.display_name();

    if let Some(bedtime_role) = cfg.role
        && !member.roles.contains(&bedtime_role)
    {
        tracing::info!("🌙 Giving {name} the bedtime role");
        member.add_role(&ctx, bedtime_role).await?;
    };

    let Some(channel) = ({
        let guild = ctx.cache.guild(member.guild_id).some()?;
        guild
            .voice_states
            .get(&member.user.id)
            .and_then(|x| x.channel_id)
            .and_then(|id| guild.channels.get(&id))
            .cloned()
    }) else {
        return Ok(());
    };

    // don't disconnect users in voice channels with specific status
    if let Some(status) = channel.status
        && let Some(re) = &cfg.ignored_vc_description
        && re.0.is_match(&status)?
    {
        return Ok(());
    };

    tracing::info!("🌙 Disconnecting {name}");
    member.guild_id.disconnect_member(&ctx, member.user.id).await?;

    Ok(())
}

async fn lift_bedtime(ctx: &Context, cfg: &ConfigT, member: Member) -> Result<()> {
    let name = member.display_name();

    if let Some(role) = cfg.role
        && member.roles.contains(&role)
    {
        tracing::info!("🌙 Removing bedtime role from {name}");
        member.remove_role(&ctx, role).await?;
    }

    Ok(())
}
