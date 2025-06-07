use bot_core::{EvtContext, OptionExt as _, State, VoiceChange, With};
use dashmap::DashMap;
use eyre::Result;
use itertools::Itertools;
use poise::serenity_prelude::all::{
    Builder, ChannelId, ChannelType, CreateChannel, GuildChannel, GuildId, VoiceState,
};
use poise::serenity_prelude::{self as serenity, Context};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigT {
    categories: Vec<ChannelId>,
}

#[derive(Default)]
pub struct StateT {
    /// 1 mutex per category
    category_locks: DashMap<serenity::ChannelId, Arc<Mutex<()>>>,
}

pub async fn voice_update(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    guild_id: GuildId,
    update: (&Option<VoiceState>, &VoiceState),
) -> Result<()> {
    let (from_id, to_id) = match VoiceChange::new(update) {
        VoiceChange::Join { to } => (to, None),
        VoiceChange::Leave { from } => (from, None),
        VoiceChange::Move { from, to } => (from, Some(to)),
        VoiceChange::Stay => return Ok(()),
    };

    let from = cloned_guild_channel(ctx.serenity_context, guild_id, from_id).some()?;
    if let Some(from_cat) = from.parent_id {
        keep_one_empty_channel(ctx, from_cat, guild_id).await?;
    }

    // If moving, also handle the category we're moving to
    if let Some(to) = to_id.and_then(|id| cloned_guild_channel(ctx.serenity_context, guild_id, id))
        && let Some(to_cat) = to.parent_id
        && Some(to_cat) != from.parent_id
    {
        keep_one_empty_channel(ctx, to_cat, guild_id).await?;
    }

    Ok(())
}

pub async fn channel_update(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    old: &Option<GuildChannel>,
    new: &GuildChannel,
) -> Result<()> {
    let old = old.as_ref().some()?;

    if old.kind != ChannelType::Voice {
        return Ok(());
    }

    if old.position == new.position {
        return Ok(());
    }

    if let Some(cat_id) = old.parent_id {
        keep_one_empty_channel(ctx, cat_id, new.guild_id).await?;
    }

    if let Some(cat_id) = new.parent_id
        && Some(cat_id) != old.parent_id
    {
        keep_one_empty_channel(ctx, cat_id, new.guild_id).await?;

        let Some(cat) = cloned_guild_channel(ctx.serenity_context, new.guild_id, cat_id) else {
            return Ok(());
        };
        let Some(channel) = cloned_guild_channel(ctx.serenity_context, new.guild_id, new.id) else {
            return Ok(());
        };

        if channel.kind == ChannelType::Voice {
            tracing::debug!("Assimilating channel to category: {} -> {}", channel.name, cat.name);
            serenity::EditChannel::default()
                .permissions(cat.permission_overwrites)
                .name(cat.name.clone())
                .execute(ctx.serenity_context, channel.id)
                .await?;
        }
    }

    Ok(())
}

async fn keep_one_empty_channel(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    category_id: ChannelId,
    guild_id: GuildId,
) -> Result<()> {
    if !ctx.user_data.with_ok(|cfg| cfg.categories.contains(&category_id)).await? {
        return Ok(());
    }

    tracing::debug!("Normalizing ephemeral voice channels in {category_id}");

    // Get exclusive access to the category so concurrent calls don't clash
    // Technically we're leaking memory if categories get deleted 🤓
    let lock = ctx.user_data.state().category_locks.entry(category_id).or_default().clone();
    let _lock = lock.lock().await;

    let cat_channels = ctx
        .serenity_context
        .cache
        .guild(guild_id)
        .some()?
        .channels
        .values()
        .filter(|c| c.kind == ChannelType::Voice && c.parent_id == Some(category_id))
        .sorted_by_key(|c| c.position)
        .map(|c| c.id)
        .collect_vec();

    let Some((last, rest)) = cat_channels.split_last() else { return Ok(()) };

    // Partition channels into dead or alive (I'd use partition() but members() returns Result)
    for &id in rest {
        if let Some(channel) = cloned_guild_channel(ctx.serenity_context, guild_id, id)
            && channel.members(ctx.serenity_context)?.is_empty()
            // could have been moved
            && channel.parent_id == Some(category_id)
        {
            channel.delete(ctx.serenity_context).await?;
        }
    }

    // Create a new channel if the last one is not empty
    if cloned_guild_channel(ctx.serenity_context, guild_id, *last)
        .is_some_and(|c| !c.members(ctx.serenity_context).unwrap().is_empty())
        && let Some(category) = cloned_guild_channel(ctx.serenity_context, guild_id, category_id)
    {
        CreateChannel::new(category.name.clone())
            .category(&category)
            .kind(ChannelType::Voice)
            .execute(ctx.serenity_context, category.guild_id)
            .await?;
    }

    Ok(())
}

fn cloned_guild_channel(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
) -> Option<GuildChannel> {
    let guild = ctx.cache.guild(guild_id)?;
    guild.channels.get(&channel_id).cloned()
}
