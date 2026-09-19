use bot_core::msg::Msg;
use eyre::{Error, Report};
use poise::serenity_prelude::{Colour, CreateEmbed, FullEvent, Interaction};
use poise::{ApplicationContext, Context, FrameworkError};

pub async fn on_error<D>(error: FrameworkError<'_, D, Error>) {
    if let Err(e) = async {
        match error {
            FrameworkError::Setup { error, .. } => {
                panic!("Failed to start bot: {error:?}");
            }
            FrameworkError::EventHandler { error, event, framework, .. } => {
                let event_name = event.snake_case_name();
                tracing::error!("Event handler error on {event_name}: {error:?}");
                if let FullEvent::InteractionCreate { interaction: Interaction::Component(component) } = event {
                    let msg = error_msg(format_report(&error));
                    // XXX: There is no way for us to know if the interaction has already been responded to,
                    // so we try to create a response first, and if that fails, we send a followup instead.
                    // This is obviously terrible.
                    if msg.clone().respond_to_component(framework.serenity_context, component).await.is_err() {
                        msg.followup_to_component(framework.serenity_context, component).await?;
                    }
                }
            }
            FrameworkError::Command { error, ctx, .. } => {
                tracing::error!("Error in /{}: {error:?}", ctx.command().name);
                let msg = error_msg(format_report(&error));
                let response = ctx.send(msg.clone().to_reply()).await;
                if response.is_err() {
                    match ctx {
                        Context::Application(ApplicationContext { interaction, .. }) => {
                            msg.followup_to_command(ctx.serenity_context(), interaction).await?;
                        }
                        Context::Prefix(_) => {
                            response?;
                        }
                    }
                }
            }
            FrameworkError::ArgumentParse { ctx, input, error, .. } => {
                tracing::warn!("Error parsing arguments: {error:?}");
                let usage = ctx.command().help_text.as_ref().map_or("", |v| v);
                let val = input.map_or("".to_string(), |v| format!("Invalid value `{v}`: "));
                ctx.send(error_msg(format!("**{val}{error}**\n{usage}")).to_reply()).await?;
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

fn error_msg(text: impl Into<String>) -> Msg {
    let mut text: String = text.into();
    text.truncate(1024);
    let embed = CreateEmbed::new().description(text).colour(Colour::RED);
    Msg::default().embed(embed).ephemeral(true)
}

fn format_report(err: &Report) -> String {
    let err_str = format!("{err:#}");
    if err_str.contains('\n') { format!("```\n{err_str:#}\n```") } else { err_str }
}
