mod log;

use crate::log::Log;
use bot_core::roles::enforce_roles;
use bot_core::{EvtContext, State, UserData, With};
use chrono::prelude::{Datelike, Local, NaiveDate};
use eyre::Result;
use poise::serenity_prelude::all::{Message, RoleId};
use poise::serenity_prelude::prelude::Context;
use poise::serenity_prelude::{GuildId, UserId, VoiceState};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

const ROLE_ADD_REMOVE_PER_MINUTE: u16 = 10;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigT {
    tracked_weeks: u16,
    roles: BTreeMap<RoleId, u16>,
    today: Option<NaiveDate>,
    log: BTreeMap<UserId, Log>,
}

pub async fn message(ctx: EvtContext<'_, impl With<ConfigT>>, message: &Message) -> Result<()> {
    set_user_as_active_today(ctx, message.author.id).await
}

pub async fn voice_update(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    _guild_id: GuildId,
    (_old, new): (&Option<VoiceState>, &VoiceState),
) -> Result<()> {
    set_user_as_active_today(ctx, new.user_id).await
}

async fn set_user_as_active_today(ctx: EvtContext<'_, impl With<ConfigT>>, user_id: UserId) -> Result<()> {
    let today = Local::now().weekday();
    if ctx
        .user_data
        .with_ok(|cfg| cfg.tracked_weeks > 0 && cfg.log.get(&user_id).is_none_or(|log| !log.get_today(today)))
        .await?
    {
        ctx.user_data.with_mut_ok(|cfg| cfg.log.entry(user_id).or_default().set_today(today)).await?;
    }
    Ok(())
}

pub async fn setup(ctx: Context, data: impl With<ConfigT> + State<GuildId>) -> Result<()> {
    tracing::debug!("Spawning activity role worker");
    {
        let ctx = ctx.clone();
        let data = data.clone();
        tokio::spawn(async move {
            loop {
                tracing::trace!("Periodic role update");
                if let Err(error) = update_roles(&ctx, &data).await {
                    tracing::error!("Error in role worker: {error:?}");
                }
                sleep(Duration::from_secs(60)).await;
            }
        });
    }
    tracing::debug!("Spawning activity log maintenance worker");
    tokio::spawn(async move {
        loop {
            tracing::trace!("Periodic log update");
            if let Err(error) = update_logs(&data).await {
                tracing::error!("Error in log worker: {error:?}");
            }
            sleep(Duration::from_secs(60)).await;
        }
    });
    Ok(())
}

async fn update_roles(ctx: &Context, data: &(impl With<ConfigT> + State<GuildId>)) -> Result<()> {
    let config = data.with_ok(|cfg| cfg.clone()).await?;
    let guild_id: GuildId = *data.state();

    let mut roles: HashMap<RoleId, HashSet<UserId>> = HashMap::new();
    for (role_id, required_days) in config.roles {
        let qualified_users: HashSet<UserId> = config
            .log
            .iter()
            .filter(|(_, log)| log.logged_days() >= required_days.into())
            .map(|(&user_id, _)| user_id)
            .collect();
        roles.insert(role_id, qualified_users);
    }

    enforce_roles(ctx, guild_id, &roles, ROLE_ADD_REMOVE_PER_MINUTE).await?;

    Ok(())
}

async fn update_logs(data: &impl With<ConfigT>) -> Result<()> {
    let today = Local::now().date_naive();
    let Some(config_day) = data.with_ok(|cfg| cfg.today).await? else {
        data.with_mut_ok(|cfg| cfg.today = Some(today)).await?;
        return Ok(());
    };

    if config_day == today {
        return Ok(());
    }

    if config_day > today {
        warn!("Uh oh, looks like we went back in time?! Resetting date...");
        data.with_mut_ok(|cfg| cfg.today = Some(today)).await?;
        return Ok(());
    }

    data.with_mut_ok(|cfg| {
        cfg.today = Some(today);
        if today.iso_week() != config_day.iso_week() {
            for log in cfg.log.values_mut() {
                log.start_new_week(cfg.tracked_weeks.into());
            }
        }
    })
    .await?;

    Ok(())
}
