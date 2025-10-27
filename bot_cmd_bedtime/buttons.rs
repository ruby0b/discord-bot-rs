use crate::ConfigT;
use bot_core::iso_weekday::IsoWeekday;
use bot_core::{CreateReplyExt, EvtContext, OptionExt as _, With};
use chrono::{Utc, Weekday};
use eyre::{OptionExt, Result, ensure};
use poise::CreateReply;
use poise::serenity_prelude::{Color, ComponentInteraction, ComponentInteractionDataKind};
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
        .with_mut(|cfg| {
            let bedtime = cfg.bedtimes.get_mut(&id).ok_or_eyre("Bedtime no longer exists")?;
            ensure!(component.user.id == bedtime.user, "That's not your own bedtime");
            if !bedtime.repeat.remove(&weekday) {
                bedtime.repeat.insert(weekday);
            }
            Ok(bedtime.clone())
        })
        .await?;

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
        .with_mut(|cfg| {
            let bedtime = cfg.bedtimes.get(&id).cloned().ok_or_eyre("Bedtime no longer exists")?;
            ensure!(component.user.id == bedtime.user, "That's not your own bedtime");
            cfg.bedtimes.remove(&id);
            Ok(bedtime)
        })
        .await?;

    let now = Utc::now();
    CreateReply::new()
        .embed(bedtime.embed(now).color(Color::DARKER_GREY))
        .components(bedtime.select_menu_component(id, ctx.user_data, now).await?)
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
        .with(|cfg| {
            let bedtime = cfg.bedtimes.get(&id).cloned().ok_or_eyre("Bedtime no longer exists")?;
            ensure!(component.user.id == bedtime.user, "That's not your own bedtime");
            Ok(bedtime)
        })
        .await?;

    bedtime
        .reply(id, ctx.user_data, Utc::now())
        .await?
        .edit_message(ctx.serenity_context, &component.message)
        .await?;

    Ok(())
}
