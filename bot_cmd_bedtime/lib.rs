/*
TODO add/remove bedtimes
/add time Option<date>
/add should display the bedtime with toggle buttons for each weekday
*/

use bot_core::interval_set::IntervalSet;
use bot_core::iso_weekday::IsoWeekday;
use bot_core::serde::LiteralRegex;
use bot_core::{OptionExt, State, With, get_member};
use chrono::{DateTime, Datelike, Days, TimeDelta, Utc};
use eyre::Result;
use poise::serenity_prelude::all::{GuildId, UserId};
use poise::serenity_prelude::prelude::Context;
use poise::serenity_prelude::{Member, RoleId};
use std::collections::{BTreeMap, BTreeSet};
use std::iter;

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
    fn currently_relevant_bedtimes(&self, now: DateTime<Utc>) -> BTreeSet<DateTime<Utc>> {
        let today = now.date_naive().and_time(self.first.time()).and_utc();
        let repeats = [today - Days::new(1), today]
            .into_iter()
            .chain((1..=7).map(|offset| today + Days::new(offset)))
            .filter(|&bedtime| {
                bedtime > self.first && self.repeat.contains(&IsoWeekday(bedtime.weekday()))
            });
        iter::once(self.first).chain(repeats).collect()
    }

    fn next(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.currently_relevant_bedtimes(now).into_iter().find(|&bedtime| bedtime > now)
    }
}

pub async fn bedtime_loop(ctx: &Context, data: &(impl With<ConfigT> + State<GuildId>)) {
    let mut interval = tokio::time::interval(std::time::Duration::from_mins(1));
    interval.tick().await;
    loop {
        if let Err(error) = enforce_and_lift_bedtimes(ctx, data).await {
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
    let role = data.with_ok(|cfg| cfg.role).await?;

    for (&user_id, intervals) in intervals_by_user.iter() {
        let Some(member) = get_member(ctx, guild_id, user_id) else { continue };
        if intervals.find(now).is_some() {
            enforce_bedtime(ctx, role, member).await?;
        } else {
            lift_bedtime(ctx, role, member).await?;
        }
    }

    Ok(())
}

async fn get_bedtime_intervals_and_prune(
    data: &impl With<ConfigT>,
    now: DateTime<Utc>,
) -> Result<BTreeMap<UserId, IntervalSet<DateTime<Utc>>>> {
    data.with_mut_ok(|cfg| {
        cfg.users
            .iter_mut()
            .map(|(&user_id, bedtimes)| {
                let intervals = bedtimes
                    .iter()
                    .flat_map(|x| x.currently_relevant_bedtimes(now))
                    .map(|x| x..(x + cfg.duration))
                    .collect::<IntervalSet<_>>();
                // prune outdated bedtimes that don't repeat
                bedtimes.retain(|x| x.first >= (now - cfg.duration) || !x.repeat.is_empty());
                (user_id, intervals)
            })
            .collect::<BTreeMap<_, _>>()
    })
    .await
}

async fn enforce_bedtime(ctx: &Context, role: Option<RoleId>, member: Member) -> Result<()> {
    let name = member.display_name();

    if let Some(bedtime_role) = role
        && !member.roles.contains(&bedtime_role)
    {
        tracing::info!("🌙 Giving {name} the bedtime role");
        member.add_role(&ctx, bedtime_role).await?;
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

async fn lift_bedtime(ctx: &Context, role: Option<RoleId>, member: Member) -> Result<()> {
    let name = member.display_name();

    if let Some(role) = role
        && member.roles.contains(&role)
    {
        tracing::info!("🌙 Removing bedtime role from {name}");
        member.remove_role(&ctx, role).await?;
    }

    Ok(())
}
