use eyre::Error;
use poise::serenity_prelude::{
    Colour, CreateEmbed, CreateInteractionResponse, FullEvent, Interaction,
};
use poise::{CreateReply, FrameworkError};

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
                    let reply = error_reply(format!("```\n{error:#}\n```"));
                    // XXX: There is no way for us to know if the interaction has already been responded to,
                    // so we try to create a response first, and if that fails, we send a followup instead.
                    // This is obviously terrible.
                    if (component
                        .create_response(
                            framework.serenity_context,
                            CreateInteractionResponse::Message(
                                reply.clone().to_slash_initial_response(Default::default()),
                            ),
                        )
                        .await)
                        .is_err()
                    {
                        component
                            .create_followup(
                                framework.serenity_context,
                                reply.to_slash_followup_response(Default::default()),
                            )
                            .await?;
                    }
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
