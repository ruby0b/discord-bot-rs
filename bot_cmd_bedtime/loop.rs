use crate::ConfigT;
use bot_core::interval_set::IntervalSet;
use bot_core::{OptionExt as _, State, With, get_member};
use chrono::{DateTime, Utc};
use eyre::Result;
use itertools::Itertools;
use poise::serenity_prelude::Member;
use poise::serenity_prelude::all::{GuildId, UserId};
use poise::serenity_prelude::prelude::Context;
use std::collections::{BTreeMap, HashSet};

pub(crate) async fn bedtime_loop(ctx: Context, data: impl With<ConfigT> + State<GuildId>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
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
    let intervals_by_user = get_bedtime_intervals(data, now).await?;
    prune_outdated_bedtimes(data, now).await?;
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

async fn get_bedtime_intervals(
    data: &impl With<ConfigT>,
    now: DateTime<Utc>,
) -> Result<BTreeMap<UserId, IntervalSet<DateTime<Utc>>>> {
    data.with_ok(|cfg| {
        cfg.bedtimes
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
            .collect::<BTreeMap<_, _>>()
    })
    .await
}

async fn prune_outdated_bedtimes(data: &impl With<ConfigT>, now: DateTime<Utc>) -> Result<()> {
    let outdated = data
        .with_ok(|cfg| {
            cfg.bedtimes
                .iter()
                .filter(|(_, b)| b.repeat.is_empty() && b.first < now - cfg.duration)
                .map(|(id, _)| *id)
                .collect::<HashSet<_>>()
        })
        .await?;

    if !outdated.is_empty() {
        data.with_mut_ok(|cfg| {
            cfg.bedtimes.retain(|id, _| !outdated.contains(id));
        })
        .await?;
    }

    Ok(())
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
