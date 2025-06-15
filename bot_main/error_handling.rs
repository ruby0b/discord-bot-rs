use eyre::Error;
use poise::serenity_prelude::{Colour, CreateEmbed, FullEvent, Interaction};
use poise::{CreateReply, FrameworkError, serenity_prelude as serenity};

pub async fn on_error<D>(error: FrameworkError<'_, D, Error>) {
    if let Err(e) = async {
        match error {
            FrameworkError::Setup { error, .. } => {
                panic!("Failed to start bot: {error:?}");
            }
            FrameworkError::EventHandler { error, event, framework, .. } => {
                let event_name = event.snake_case_name();
                tracing::error!("Event handler error on {event_name}: {error:?}");
                if let FullEvent::InteractionCreate {
                    interaction: Interaction::Component(component),
                } = event
                {
                    component
                        .create_response(
                            framework.serenity_context,
                            serenity::CreateInteractionResponse::Message(
                                error_reply(format!("```\n{error:#}\n```"))
                                    .to_slash_initial_response(Default::default()),
                            ),
                        )
                        .await?;
                }
            }
            FrameworkError::Command { error, ctx, .. } => {
                tracing::error!("Error in /{}: {error:?}", ctx.command().name);
                ctx.send(error_reply(format!("```\n{error:#}\n```"))).await?;
            }
            FrameworkError::ArgumentParse { ctx, input, error, .. } => {
                tracing::warn!("Error parsing arguments: {error:?}");
                let usage = ctx.command().help_text.as_ref().map_or("", |v| v);
                let val = input.map_or("".to_string(), |v| format!("Invalid value `{v}`: "));
                ctx.send(error_reply(format!("**{val}{error}**\n{usage}"))).await?;
            }
            error => {
                poise::builtins::on_error(error).await?;
            }
        };
        eyre::Ok(())
    }
    .await
    {
        tracing::error!("Error while handling error: {e:?}")
    }
}

fn error_reply(text: impl Into<String>) -> CreateReply {
    let mut text: String = text.into();
    text.truncate(1024);
    let embed = CreateEmbed::new().description(text).colour(Colour::RED);
    CreateReply::default().embed(embed).ephemeral(true)
}
