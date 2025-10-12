#![feature(trait_alias)]

mod weekly_time;

use crate::weekly_time::WeeklyTime;
use bot_core::serde::LiteralRegex;
use bot_core::timer_queue::{TimerCommand, spawn_timer_queue};
use bot_core::{CmdContext, EvtContext, OptionExt, State, VoiceChange, With, safe_name};
use chrono::{DateTime, NaiveWeek, TimeDelta, Utc, Weekday};
use dashmap::DashMap;
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::all::{
    Builder, ChannelId, ChannelType, EditChannel, GuildId, UserId, VoiceState,
};
use poise::serenity_prelude::prelude::Context;
use poise::serenity_prelude::{Mentionable, RoleId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::{OnceCell, mpsc};

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

#[derive(Default)]
pub struct StateT {
    timer_sender: OnceCell<mpsc::Sender<TimerCommand<(UserId, Bedtime)>>>,
}

pub async fn bedtime_loop(
    ctx: Context,
    data: impl With<ConfigT> + State<StateT> + State<GuildId>,
) -> Result<()> {
    let state: Arc<StateT> = data.state();
    let tx = spawn_timer_queue(move |bedtime| enforce_bedtime(ctx.clone(), data.clone(), bedtime));
    state.timer_sender.set(tx)?;
    Ok(())
}

async fn enforce_bedtime(
    ctx: Context,
    data: impl With<ConfigT> + State<StateT> + State<GuildId>,
    (user_id, bedtime): (UserId, Bedtime),
) -> Result<()> {
    let guild_id: GuildId = *data.state();
    let name = safe_name(&ctx, &user_id);
    let member = {
        let guild = ctx.cache.guild(guild_id).some()?;
        guild.members.get(&user_id).ok_or_eyre("guild member not found")?.clone()
    };

    if let Some(bedtime_role_id) = data.with_ok(|cfg| cfg.role).await? {
        tracing::info!("🌙 Giving {name} the bedtime role");
        member.add_role(&ctx, bedtime_role_id).await?;
    };

    if let Some(channel_id) = {
        let guild = ctx.cache.guild(guild_id).some()?;
        guild.voice_states.get(&user_id).and_then(|x| x.channel_id)
    } {
        tracing::info!("🌙 Disconnecting {name}");
        guild_id.disconnect_member(&ctx, user_id).await?;
    }

    Ok(())
}
