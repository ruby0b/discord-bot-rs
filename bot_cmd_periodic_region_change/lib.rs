use anyhow::{Context as _, Result};
use chrono::TimeDelta;
use bot_core::serde::LiteralRegex;
use bot_core::{CmdContext, EvtContext, State, VoiceChange, With};
use dashmap::DashMap;
use poise::serenity_prelude::all::{
    Builder, ChannelId, ChannelType, EditChannel, GuildId, UserId, VoiceState,
};
use poise::serenity_prelude::prelude::Context;
use std::collections::BTreeSet;

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
    handles: DashMap<UserId, tokio::task::JoinHandle<()>>,
}

/// Set the alternative server region for the auto region change feature
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn auto_region_change<D: With<ConfigT>>(
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
        VoiceChange::Leave { .. } => {
            if let Some((_, old_handle)) = ctx.user_data.state().handles.remove(&user_id) {
                tracing::debug!("Aborting region change task for {user_id} because they left");
                old_handle.abort();
            }
        }
        VoiceChange::Join { to } | VoiceChange::Move { to, .. } => {
            let Some(cooldown) = ctx
                .user_data
                .with_ok(|cfg| cfg.users.contains(&user_id).then_some(cfg.cooldown))
                .await?
            else {
                return Ok(());
            };

            if let Some((_, old_handle)) = ctx.user_data.state().handles.remove(&user_id) {
                tracing::debug!("Aborting old region change task for {user_id} because they moved");
                old_handle.abort();
            }

            let handle = {
                let data = ctx.user_data.clone();
                let ctx = ctx.serenity_context.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(cooldown.to_std().unwrap_or_default()).await;
                        if let Err(e) = change_region(&ctx, &data, guild_id, user_id, to).await {
                            tracing::error!("Error while changing region for {user_id}: {e:?}");
                        }
                    }
                })
            };

            ctx.user_data.state().handles.insert(user_id, handle);
        }
        VoiceChange::Stay => (),
    }

    Ok(())
}

async fn change_region(
    ctx: &Context,
    data: &impl With<ConfigT>,
    guild_id: GuildId,
    user_id: UserId,
    vc_id: ChannelId,
) -> Result<()> {
    let Some(config) =
        data.with_ok(|cfg| cfg.users.contains(&user_id).then(|| cfg.clone())).await?
    else {
        return Ok(());
    };

    let vc = {
        let guild = guild_id.to_guild_cached(&ctx).context("uncached guild")?;
        let Some(voice_state) = guild.voice_states.get(&user_id) else { return Ok(()) };
        let Some(vc_id) = voice_state.channel_id else { return Ok(()) };
        guild.channels.get(&vc_id).context("uncached channel")?.clone()
    };

    if vc.id != vc_id || vc.kind != ChannelType::Voice {
        return Ok(());
    }

    if let Some(status) = vc.status
        && let Some(re) = config.ignored_vc_description
        && re.0.is_match(&status)?
    {
        tracing::info!("Not changing voice channel region because of status: {status}");
        return Ok(());
    }

    // toggle between Automatic (None) and the configured alternative region
    let new_region = if vc.rtc_region.is_some() { None } else { Some(config.region) };

    tracing::debug!("Changing voice channel region to {new_region:?} for {user_id}");

    EditChannel::new()
        .voice_region(new_region)
        .audit_log_reason("auto region change")
        .execute(ctx, vc.id)
        .await?;

    Ok(())
}
