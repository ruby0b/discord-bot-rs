use bot_core::iso_week::IsoWeekSerde;
use bot_core::roles::enforce_roles;
use bot_core::{EvtContext, State, UserData, With};
use chrono::prelude::{Datelike, Local};
use chrono::Days;
use eyre::Result;
use poise::serenity_prelude::all::{Message, RoleId};
use poise::serenity_prelude::prelude::Context;
use poise::serenity_prelude::{GuildId, UserId, VoiceState};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Duration;
use tokio::time::sleep;

const ROLE_ADD_REMOVE_PER_MINUTE: u16 = 10;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigT {
    week_range: u16,
    roles: BTreeMap<RoleId, u16>,
    active_weeks: BTreeMap<UserId, BTreeSet<IsoWeekSerde>>,
}

pub async fn message(ctx: EvtContext<'_, impl With<ConfigT>>, message: &Message) -> Result<()> {
    mark_user_as_active_this_week(ctx, message.author.id).await
}

pub async fn voice_update(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    _guild_id: GuildId,
    (_old, new): (&Option<VoiceState>, &VoiceState),
) -> Result<()> {
    mark_user_as_active_this_week(ctx, new.user_id).await
}

async fn mark_user_as_active_this_week(ctx: EvtContext<'_, impl With<ConfigT>>, user_id: UserId) -> Result<()> {
    let now = IsoWeekSerde(Local::now().date_naive().iso_week());
    if ctx.user_data.with_ok(|cfg| cfg.active_weeks.get(&user_id).is_none_or(|weeks| !weeks.contains(&now))).await? {
        ctx.user_data.with_mut_ok(|cfg| cfg.active_weeks.entry(user_id).or_default().insert(now)).await?;
    }
    Ok(())
}

pub async fn setup(ctx: Context, data: impl With<ConfigT> + State<GuildId>) -> Result<()> {
    tracing::debug!("Spawning activity role worker");
    tokio::spawn(async move {
        loop {
            tracing::trace!("Periodic update");
            if let Err(error) = update(&ctx, &data).await {
                tracing::error!("Error in worker: {error:?}");
            }
            sleep(Duration::from_secs(60)).await;
        }
    });
    Ok(())
}

async fn update(ctx: &Context, data: &(impl With<ConfigT> + State<GuildId>)) -> Result<()> {
    let config = data.with_ok(|cfg| cfg.clone()).await?;
    let guild_id: GuildId = *data.state();

    // trim active weeks and update config if necessary
    let mut active_weeks = config.active_weeks;
    {
        let day_range = Days::new((config.week_range * 7).into());
        let oldest_week = IsoWeekSerde((Local::now().date_naive() - day_range).iso_week());
        let mut any_change = false;
        for weeks in active_weeks.values_mut() {
            let trimmed_weeks = weeks.split_off(&oldest_week);
            if !weeks.is_empty() {
                any_change = true;
            }
            *weeks = trimmed_weeks;
        }
        if any_change {
            data.with_mut_ok(|cfg| cfg.active_weeks = active_weeks.clone()).await?;
        }
    }

    let mut roles: HashMap<RoleId, HashSet<UserId>> = HashMap::new();
    for (role_id, required_weeks) in config.roles {
        let qualified_users: HashSet<UserId> = active_weeks
            .iter()
            .filter(|(_, weeks)| weeks.len() >= required_weeks.into())
            .map(|(&user_id, _)| user_id)
            .collect();
        roles.insert(role_id, qualified_users);
    }

    enforce_roles(ctx, guild_id, &roles, ROLE_ADD_REMOVE_PER_MINUTE).await?;

    Ok(())
}
