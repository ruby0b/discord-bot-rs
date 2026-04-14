use bot_core::ext::option::OptionExt as _;
use bot_core::serde::LiteralRegex;
use bot_core::voice_change::VoiceChange;
use bot_core::{CmdContext, EvtContext, State, With};
use chrono::TimeDelta;
use dashmap::{DashMap, Entry};
use eyre::{OptionExt as _, Result};
use itertools::Itertools;
use poise::serenity_prelude::Guild;
use poise::serenity_prelude::all::{Builder, ChannelId, ChannelType, EditChannel, GuildId, UserId, VoiceState};
use poise::serenity_prelude::prelude::Context;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    #[serde(with = "bot_core::serde::td_seconds")]
    #[default(TimeDelta::minutes(60))]
    cooldown: TimeDelta,
    region: String,
    ignored_vc_description: Option<LiteralRegex>,
    users: BTreeSet<UserId>,
}

#[derive(Default)]
pub struct StateT {
    last_region_change: DashMap<UserId, RegionChange>,
}

struct RegionChange {
    when: Instant,
    region: Option<String>,
}

/// Set the alternative server region for the auto region change feature
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn periodic_region_change<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Server region"]
    #[autocomplete = "bot_core::autocomplete::voice_region"]
    region: String,
) -> Result<()> {
    ctx.data().with_mut_ok(|cfg| cfg.region = region.clone()).await?;
    ctx.say(format!("Set alternative region to `{region}`")).await?;
    Ok(())
}

pub async fn voice_update<D: With<ConfigT> + State<StateT>>(
    ctx: EvtContext<'_, D>,
    guild_id: GuildId,
    (old, new): (&Option<VoiceState>, &VoiceState),
) -> Result<()> {
    let user_id = new.user_id;

    match VoiceChange::new((old, new)) {
        // track users joining/moving to channels with a different region
        VoiceChange::Join { to } | VoiceChange::Move { to, .. } => {
            if ctx.user_data.with_ok(|cfg| !cfg.users.contains(&user_id)).await? {
                return Ok(());
            };

            let now = Instant::now();
            let region =
                ctx.serenity_context.cache.guild(guild_id).some()?.channels.get(&to).some()?.rtc_region.clone();
            match ctx.user_data.state().last_region_change.entry(user_id) {
                Entry::Occupied(mut entry) => {
                    let last = entry.get_mut();
                    if region != last.region {
                        *last = RegionChange { when: now, region };
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(RegionChange { when: now, region });
                }
            }
        }
        VoiceChange::Leave { .. } | VoiceChange::Stay => (),
    }

    Ok(())
}

async fn periodic_region_change_loop(ctx: Context, data: impl With<ConfigT> + State<GuildId> + State<StateT>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.tick().await;
    loop {
        if let Err(error) = periodic_region_changes(&ctx, &data).await {
            tracing::error!("Error in periodic region change loop: {error:?}");
        }
        interval.tick().await;
    }
}

async fn periodic_region_changes(
    ctx: &Context,
    data: &(impl With<ConfigT> + State<GuildId> + State<StateT>),
) -> Result<()> {
    let config = data.with_ok(|c| c.clone()).await?;
    let guild_id: GuildId = *data.state();
    let configured_users_by_vc = {
        let guild = guild_id.to_guild_cached(&ctx).some()?;
        config
            .users
            .iter()
            .map(|&u| get_user_voice_channel(&guild, u).map(|x| x.map(|vc| (vc, u))))
            .filter_map(Result::transpose)
            .collect::<Result<Vec<(ChannelId, UserId)>>>()?
            .into_iter()
            .into_group_map()
    };
    for (channel_id, user_ids) in configured_users_by_vc {
        let state: Arc<StateT> = data.state();
        let elapsed = user_ids
            .iter()
            .filter_map(|u| state.last_region_change.get(u))
            .map(|x| x.when.elapsed())
            .max()
            .unwrap_or(Duration::MAX);
        if elapsed < config.cooldown.to_std()? {
            continue;
        }
        if let DidRegionChange::YesTo(region) = change_region(ctx, &config, guild_id, channel_id).await? {
            let now = Instant::now();
            for user_id in user_ids {
                state.last_region_change.insert(user_id, RegionChange { when: now, region: region.clone() });
            }
        }
    }
    Ok(())
}

enum DidRegionChange {
    YesTo(Option<String>),
    No,
}

async fn change_region(
    ctx: &Context,
    config: &ConfigT,
    guild_id: GuildId,
    vc_id: ChannelId,
) -> Result<DidRegionChange> {
    let channel = {
        let guild = guild_id.to_guild_cached(&ctx).some()?;
        guild.channels.get(&vc_id).ok_or_eyre("uncached channel")?.clone()
    };

    if channel.kind != ChannelType::Voice {
        tracing::error!("Unexpected channel type, should be Voice but was {:?}", channel.kind);
        return Ok(DidRegionChange::No);
    }

    if let Some(status) = channel.status
        && let Some(re) = &config.ignored_vc_description
        && re.0.is_match(&status)?
    {
        tracing::trace!("Not changing VC region because of status: {status}");
        return Ok(DidRegionChange::No);
    }

    // toggle between Automatic (None) and the configured alternative region
    let new_region = if channel.rtc_region.is_some() { None } else { Some(config.region.clone()) };

    tracing::debug!("Changing VC region to {new_region:?} in {} ({})", channel.name, channel.id);

    EditChannel::new()
        .voice_region(new_region.clone())
        .audit_log_reason("auto region change")
        .execute(ctx, channel.id)
        .await?;

    Ok(DidRegionChange::YesTo(new_region))
}

fn get_user_voice_channel(guild: &Guild, user_id: UserId) -> Result<Option<ChannelId>> {
    let Some(voice_state) = guild.voice_states.get(&user_id) else { return Ok(None) };
    let Some(vc_id) = voice_state.channel_id else { return Ok(None) };
    Ok(Some(vc_id))
}

pub async fn setup(ctx: Context, data: impl With<ConfigT> + State<StateT> + State<GuildId>) -> Result<()> {
    tracing::debug!("Spawning periodic region change loop");
    tokio::spawn(periodic_region_change_loop(ctx, data));
    Ok(())
}
