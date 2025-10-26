use crate::ConfigT;
use bot_core::iso_weekday::IsoWeekday;
use bot_core::{CreateReplyExt, EvtContext, OptionExt as _, With};
use chrono::{Utc, Weekday};
use eyre::{OptionExt, Result};
use poise::CreateReply;
use poise::serenity_prelude::{ComponentInteraction, ComponentInteractionDataKind};
use uuid::Uuid;

pub async fn toggle_weekday_button(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let (id_str, weekday_str) = param.split_once(":").ok_or_eyre("Expected a weekday parameter")?;
    let id = Uuid::try_parse(id_str)?;
    let weekday = IsoWeekday(weekday_str.parse::<Weekday>()?);

    component.defer(ctx.serenity_context).await?;

    tracing::info!("Toggling {} on bedtime {id}", weekday.0);
    let bedtime = ctx
        .user_data
        .with_mut_ok(|cfg| {
            cfg.bedtimes.get_mut(&id).map(|b| {
                if !b.repeat.remove(&weekday) {
                    b.repeat.insert(weekday);
                }
                b.clone()
            })
        })
        .await?
        .ok_or_eyre("Bedtime no longer exists")?;

    bedtime
        .reply(id, ctx.user_data, Utc::now())
        .await?
        .edit_message(ctx.serenity_context, &component.message)
        .await?;

    Ok(())
}

pub async fn delete_button(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let id = Uuid::try_parse(param)?;

    component.defer(ctx.serenity_context).await?;

    tracing::info!("Removing bedtime {id}");
    let bedtime = ctx
        .user_data
        .with_mut_ok(|cfg| cfg.bedtimes.remove(&id))
        .await?
        .ok_or_eyre("Bedtime no longer exists")?;

    let now = Utc::now();
    CreateReply::new()
        .embed(bedtime.embed(now))
        .components(bedtime.components(id, ctx.user_data, now).await?)
        .edit_message(ctx.serenity_context, &component.message)
        .await?;

    Ok(())
}

pub async fn select_bedtime(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
) -> Result<()> {
    let ComponentInteractionDataKind::StringSelect { values } = &component.data.kind else {
        return Ok(());
    };

    let id = Uuid::try_parse(values.first().some()?)?;

    component.defer(ctx.serenity_context).await?;

    let bedtime = ctx
        .user_data
        .with_mut_ok(|cfg| cfg.bedtimes.remove(&id))
        .await?
        .ok_or_eyre("Bedtime no longer exists")?;

    bedtime
        .reply(id, ctx.user_data, Utc::now())
        .await?
        .edit_message(ctx.serenity_context, &component.message)
        .await?;

    Ok(())
}
