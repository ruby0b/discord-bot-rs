use crate::ConfigT;
use bot_core::iso_weekday::IsoWeekday;
use bot_core::{EvtContext, With};
use chrono::Weekday;
use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{Builder as _, Color, ComponentInteraction, EditMessage};
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

    EditMessage::new()
        .embed(bedtime.embed())
        .components(bedtime.components(id))
        .execute(ctx.serenity_context, (component.message.channel_id, component.message.id, None))
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

    EditMessage::new()
        .embed(bedtime.embed().color(Color::DARKER_GREY))
        .components(vec![])
        .execute(ctx.serenity_context, (component.message.channel_id, component.message.id, None))
        .await?;

    Ok(())
}
