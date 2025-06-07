#![feature(trait_alias)]
#![feature(anonymous_lifetime_in_impl_trait)]

mod cmd;
mod config;
mod data;
mod error_handling;
mod log;
mod message_file;
mod util;

use anyhow::{Context, Result};
use bot_core::OptionExt as _;
use poise::serenity_prelude::{
    ChannelId, Client, FullEvent, GatewayIntents, GuildId, Interaction, Settings,
};
use songbird::SerenityInit as _;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    crate::log::init_logging();

    // Read required config from environment variables (or .env file)
    let config_url = dotenv::var("BOT_CONFIG_CHANNEL").context("BOT_CONFIG_CHANNEL not set")?;
    let token = dotenv::var("BOT_TOKEN").context("BOT_TOKEN not set")?;

    let (cfg_guild, cfg_channel): (GuildId, ChannelId) = (|| -> Result<_> {
        let url = url::Url::parse(&config_url)?;
        let mut segments = url.path_segments().some()?.skip(1);
        Ok((segments.next().some()?.parse()?, segments.next().some()?.parse()?))
    })()
    .context("Config URL must be a channel link")?;

    let options = poise::FrameworkOptions {
        commands: vec![
            cmd::register(),
            cmd::reregister(),
            config::config(),
            config::restore(),
            bot_cmd_ask::ask(),
            bot_cmd_ask::delete_ask_defaults(),
            bot_cmd_ask::new_ask_defaults(),
            bot_cmd_eval::d2(),
            bot_cmd_eval::math(),
            bot_cmd_eval::typst(),
            bot_cmd_periodic_region_change::auto_region_change(),
            bot_cmd_roles::role_button(),
        ],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("=".into()),
            ..Default::default()
        },
        on_error: |error| Box::pin(async { error_handling::on_error(error).await }),
        pre_command: |ctx| {
            Box::pin(async move {
                tracing::info!("{} used /{}", ctx.author().name, ctx.command().qualified_name);
            })
        },
        event_handler: |framework, event| {
            Box::pin(async move {
                match event {
                    FullEvent::VoiceStateUpdate { old, new } => {
                        let Some(guild_id) = new.guild_id else { return Ok(()) };
                        bot_cmd_tts::voice_update(framework, guild_id, (old, new)).await?;
                        bot_cmd_ephemeral_voice_channels::voice_update(
                            framework,
                            guild_id,
                            (old, new),
                        )
                        .await?;
                        bot_cmd_periodic_region_change::voice_update(
                            framework,
                            guild_id,
                            (old, new),
                        )
                        .await?;
                    }
                    FullEvent::ChannelUpdate { old, new } => {
                        bot_cmd_ephemeral_voice_channels::channel_update(framework, old, new)
                            .await?;
                    }
                    FullEvent::PresenceUpdate { new_data, .. } => {
                        bot_cmd_tts::presence_update(framework, new_data).await?;
                        bot_cmd_tts::presence_update(framework, new_data).await?;
                    }
                    FullEvent::Message { new_message } => {
                        bot_cmd_role_icon::message(framework, new_message).await?;
                    }
                    FullEvent::InteractionCreate {
                        interaction: Interaction::Component(component),
                    } => match component.data.custom_id.as_ref() {
                        bot_cmd_ask::JOIN_BUTTON_ID => {
                            bot_cmd_ask::button_pressed(
                                framework,
                                component,
                                bot_cmd_ask::JoinOrLeave::Join,
                            )
                            .await?;
                        }
                        bot_cmd_ask::LEAVE_BUTTON_ID => {
                            bot_cmd_ask::button_pressed(
                                framework,
                                component,
                                bot_cmd_ask::JoinOrLeave::Leave,
                            )
                            .await?;
                        }
                        bot_cmd_ask::LEAVE_SERVER_BUTTON_ID => {
                            bot_cmd_ask::leave_server(framework, component).await?;
                        }
                        bot_cmd_roles::SHOW_ROLE_SELECTION_ID => {
                            bot_cmd_roles::show_role_selection(framework, component).await?;
                        }
                        unknown_id => {
                            tracing::warn!("Unknown component interaction: {unknown_id}");
                        }
                    },
                    _ => {}
                }
                Ok(())
            })
        },
        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .setup(move |ctx, ready, _framework| {
            Box::pin(async move {
                tracing::info!("Logged in as {}", ready.user.name);

                let data = data::BotData::default();

                let config: &Arc<crate::config::Config<_>> = data.as_ref();
                tokio::spawn(config.clone().write_periodically(ctx.clone()));
                config.init((&ctx.cache, &ctx.http), Some(cfg_guild), cfg_channel).await?;

                tracing::debug!("Pre-fetching TTS clips");
                bot_cmd_tts::get_clips(ctx, &data.clone()).await?;

                bot_core::hash_store::purge_expired().await?;

                bot_cmd_ask::load_asks(ctx, &data).await?;

                Ok(data)
            })
        })
        .options(options)
        .build();

    Client::builder(token, GatewayIntents::all())
        .framework(framework)
        .cache_settings(Settings::default())
        .register_songbird()
        .await
        .context("Err creating client")?
        .start()
        .await
        .context("Client error")
}
