use anyhow::Error;
use bot_core::CmdContext;
use poise::serenity_prelude::{Colour, CreateEmbed};
use poise::{CreateReply, FrameworkError, serenity_prelude as serenity};

pub async fn on_error<D>(error: FrameworkError<'_, D, Error>) {
    if let Err(e) = async {
        match error {
            FrameworkError::Setup { error, .. } => {
                panic!("Failed to start bot: {error:?}");
            }
            FrameworkError::EventHandler { error, event, .. } => {
                let event_name = event.snake_case_name();
                tracing::error!("Event handler error on {event_name}: {error:?}");
            }
            FrameworkError::Command { error, ctx, .. } => {
                tracing::error!("Error in /{}: {error:?}", ctx.command().name);
                reply_error(ctx, format!("Error: {error:#}")).await?;
            }
            FrameworkError::ArgumentParse { ctx, input, error, .. } => {
                tracing::warn!("Error parsing arguments: {error:?}");
                let usage = ctx.command().help_text.as_ref().map_or("", |v| v);
                let val = input.map_or("".to_string(), |v| format!("Invalid value `{v}`: "));
                reply_error(ctx, format!("**{val}{error}**\n{usage}")).await?;
            }
            error => {
                poise::builtins::on_error(error).await?;
            }
        };
        anyhow::Ok(())
    }
    .await
    {
        tracing::error!("Error while handling error: {e:?}")
    }
}

async fn reply_error<D>(
    ctx: CmdContext<'_, D>,
    text: impl Into<String>,
) -> Result<(), serenity::Error> {
    let mut text: String = text.into();
    text.truncate(1024);
    let embed = CreateEmbed::new().description(text).colour(Colour::RED);
    ctx.send(CreateReply::default().embed(embed).ephemeral(true)).await?;
    Ok(())
}
