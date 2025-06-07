#![feature(btree_extract_if)]
#![allow(clippy::mutable_key_type)]

use bot_core::serde::LiteralRegex;
use bot_core::{CmdContext, EvtContext, With};
use chrono::{DateTime, Local, NaiveDateTime, TimeDelta, TimeZone as _, Utc};
use eyre::{OptionExt as _, Result, WrapErr as _};
use fancy_regex::Regex;
use itertools::Itertools;
use poise::serenity_prelude::{
    self as serenity, Builder, Colour, ComponentInteraction, Context, CreateActionRow,
    CreateAllowedMentions, CreateInteractionResponse, CreateInteractionResponseMessage,
    CreateMessage, Mentionable as _, MessageId,
};
use std::collections::BTreeMap;
use std::time::Duration;
use tracing::warn;
use url::Url;

pub const JOIN_BUTTON_ID: &str = "ask.join_button";
pub const LEAVE_BUTTON_ID: &str = "ask.leave_button";
pub const LEAVE_SERVER_BUTTON_ID: &str = "ask.leave_server";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    #[serde(with = "bot_core::serde::td_seconds")]
    #[default(TimeDelta::hours(3))]
    expiration: TimeDelta,
    defaults: BTreeMap<LiteralRegex, AskDefaults>,
    asks: BTreeMap<MessageId, Ask>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
struct AskDefaults {
    min_players: Option<u32>,
    max_players: Option<u32>,
    url: Option<Url>,
    description: Option<String>,
    thumbnail_url: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
struct Ask {
    players: Vec<serenity::UserId>,
    min_players: Option<u32>,
    max_players: Option<u32>,
    title: String,
    url: Option<Url>,
    description: Option<String>,
    thumbnail_url: Option<String>,
    channel_id: serenity::ChannelId,
    role_id: Option<serenity::RoleId>,
    #[serde(with = "chrono::serde::ts_seconds")]
    start_time: DateTime<Utc>,
    pinged: bool,
}

impl Ask {
    fn edit_message(&self) -> serenity::EditMessage {
        serenity::EditMessage::new()
            .content(self.content())
            .embed(self.embed())
            .allowed_mentions(CreateAllowedMentions::new().roles(self.role_id))
    }

    fn content(&self) -> String {
        self.role_id.map(|r| r.mention().to_string()).unwrap_or_default()
    }

    fn embed(&self) -> serenity::CreateEmbed {
        let min = self.min_players.map(|x| x.to_string()).unwrap_or("0".to_string());
        let max = self.max_players.map(|x| x.to_string()).unwrap_or("∞".to_string());

        let embed = serenity::CreateEmbed::default()
            .title(self.title.clone())
            .colour(if self.full() {
                Colour::BLUE
            } else if self.start_time > Utc::now() {
                Colour::GOLD
            } else {
                Colour::DARK_GREEN
            })
            .field("Min Players", min, true)
            .field("Max Players", max, true)
            .fields((!self.has_started()).then(|| {
                let unix = self.start_time.timestamp();
                ("Starts", format!("<t:{unix}:R>"), false)
            }))
            .field(format!("Players: {}", self.players.len()), self.player_mentions(), false);
        let embed = match &self.description {
            Some(description) => embed.description(description),
            None => embed,
        };
        let embed = match &self.url {
            Some(url) => embed.url(url.clone()),
            None => embed,
        };
        let embed = match &self.thumbnail_url {
            Some(url) => embed.thumbnail(url.clone()),
            None => embed,
        };
        embed
    }

    fn full(&self) -> bool {
        self.max_players.is_some_and(|x| x as usize == self.players.len())
    }

    fn has_started(&self) -> bool {
        let delta = self.start_time.signed_duration_since(Utc::now());
        delta < TimeDelta::seconds(3)
    }

    fn player_mentions(&self) -> String {
        self.players.iter().map(|p| p.mention().to_string()).collect::<Vec<_>>().join(" ")
    }

    fn action_row(&self) -> serenity::CreateActionRow {
        let join_button = serenity::CreateButton::new(JOIN_BUTTON_ID)
            .style(serenity::ButtonStyle::Success)
            .disabled(self.full())
            .label("JOIN");
        let leave_button = serenity::CreateButton::new(LEAVE_BUTTON_ID)
            .style(serenity::ButtonStyle::Secondary)
            .label("LEAVE");
        serenity::CreateActionRow::Buttons(vec![join_button, leave_button])
    }

    fn ping(&mut self, msg_id: MessageId) -> Option<CreateMessage> {
        if !self.pinged
            && self.has_started()
            && self.players.len() >= self.min_players.unwrap_or(u32::MAX) as usize
        {
            self.pinged = true;
            if Utc::now() - self.start_time < TimeDelta::hours(1) {
                Some(
                    CreateMessage::new().reference_message((self.channel_id, msg_id)).content(
                        format!("**Lobby readyyyyy!!!!!!!!**\n-# {}", self.player_mentions()),
                    ),
                )
            } else {
                warn!("Lobby is more than 1 hour late, not pinging");
                None
            }
        } else {
            None
        }
    }
}

pub async fn load_asks(ctx: &Context, data: &impl With<ConfigT>) -> Result<()> {
    tracing::debug!("Loading asks from config");
    let ask_config = data.with_ok(|cfg| cfg.clone()).await?;
    for (msg_id, ask) in ask_config.asks {
        schedule_ask_updates(ctx, data, &ask, msg_id, ask_config.expiration).await;
    }
    Ok(())
}

/// Find players to play a game with you
#[poise::command(slash_command)]
pub async fn ask<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Game title"] title: String,
    #[description = "Link to the game"] url: Option<Url>,
    #[description = "Minimum number of players"] min_players: Option<u32>,
    #[description = "Maximum number of players"] max_players: Option<u32>,
    #[description = "Start time"]
    #[autocomplete = bot_core::autocomplete::time]
    start_time: Option<chrono::NaiveTime>,
) -> Result<()> {
    let (default, expiration) = ctx
        .data()
        .with(|cfg| {
            let mut default = None;
            for (k, v) in cfg.defaults.iter() {
                if k.0.is_match(&title)? {
                    default = Some(v.clone());
                    break;
                }
            }
            Ok((default, cfg.expiration))
        })
        .await?;
    let default = default.as_ref();

    let role_id = async {
        let category_name = ctx.guild_channel().await?.parent_id?.name(ctx).await.ok()?;
        let guild = ctx.guild()?;
        Some(guild.role_by_name(&category_name)?.id)
    }
    .await;

    let ask = Ask {
        players: vec![ctx.author().id],
        min_players: min_players.or(default.and_then(|d| d.min_players)),
        max_players: max_players.or(default.and_then(|d| d.max_players)),
        title,
        url: url.or(default.and_then(|d| d.url.clone())),
        description: default.and_then(|d| d.description.clone()),
        thumbnail_url: default.and_then(|d| d.thumbnail_url.clone()),
        channel_id: ctx.channel_id(),
        role_id,
        start_time: start_time
            .and_then(naive_time_to_next_datetime)
            .map_or_else(Utc::now, |dt| dt.to_utc()),
        pinged: false,
    };

    let msg_id = {
        let reply = poise::CreateReply::default()
            .content(format!("{} {}", ask.title, ask.content()))
            .embed(ask.embed())
            .allowed_mentions(CreateAllowedMentions::new().roles(ask.role_id))
            .components(vec![ask.action_row()]);
        let relpy_handle = ctx.send(reply).await?;
        relpy_handle.message().await?.id
    };

    ctx.data().with_mut_ok(|cfg| cfg.asks.insert(msg_id, ask.clone())).await?;

    schedule_ask_updates(ctx.serenity_context(), ctx.data(), &ask, msg_id, expiration).await;

    Ok(())
}

fn naive_time_to_next_datetime(naive_time: chrono::NaiveTime) -> Option<DateTime<Local>> {
    let now = Utc::now().naive_local();
    let date = if naive_time > now.time() { now.date() } else { now.date().succ_opt().unwrap() };
    Local.from_local_datetime(&NaiveDateTime::new(date, naive_time)).single()
}

async fn schedule_ask_updates(
    ctx: &Context,
    data: &impl With<ConfigT>,
    ask: &Ask,
    msg_id: MessageId,
    expiration: TimeDelta,
) {
    let start = ask.start_time.signed_duration_since(Utc::now()).to_std().unwrap_or_default();
    spawn(ctx.clone(), data.clone(), async move |ctx, data| {
        tokio::time::sleep(start).await;
        update_ask_message(&ctx, &data, msg_id).await
    });

    let disable = (expiration + (ask.start_time - Utc::now())).to_std().unwrap_or_default();
    spawn(ctx.clone(), data.clone(), async move |ctx, data| {
        tokio::time::sleep(disable).await;
        disable_ask_message(&ctx, &data, msg_id).await
    });

    if ask.thumbnail_url.is_none() {
        spawn(ctx.clone(), data.clone(), async move |ctx, data| {
            fetch_game_thumbnail(&ctx, &data, msg_id).await
        });
    }

    if ask.description.is_none() {
        spawn(ctx.clone(), data.clone(), async move |ctx, data| {
            fetch_game_description(&ctx, &data, msg_id).await
        });
    }
}

fn spawn<F, R, D: Sync + Send + 'static>(
    ctx: Context,
    data: D,
    future: impl FnOnce(Context, D) -> F + Send + 'static,
) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = Result<R>> + Send + 'static,
    R: Send + 'static,
{
    tokio::spawn(async move {
        if let Err(e) = future(ctx, data).await {
            tracing::error!("Error in task: {e:?}");
        };
    })
}

/// Search for a thumbnail for the ask message and update it
async fn fetch_game_thumbnail(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    let (channel_id, thumbnail_url) = {
        let Some(ask) = data.with_ok(|cfg| cfg.asks.get(&msg_id).cloned()).await? else {
            return Ok(());
        };
        if ask.thumbnail_url.is_some() {
            return Ok(());
        }
        let query = match ask.url {
            Some(url) => format!("{} site:{}", ask.title, url),
            None => format!("{} Game", ask.title),
        };
        let url = image_search::urls(image_search::Arguments::new(&query, 1))
            .await
            .ok()
            .and_then(|x| x.first().cloned());
        (ask.channel_id, url)
    };

    let Some(edit) = data
        .with_mut_ok(|cfg| {
            let ask = cfg.asks.get_mut(&msg_id)?;
            ask.thumbnail_url = thumbnail_url;
            Some(ask.edit_message())
        })
        .await?
    else {
        return Ok(());
    };

    edit.execute(ctx, (channel_id, msg_id, None)).await?;

    Ok(())
}

/// Fetch a description for the game and update the ask message
async fn fetch_game_description(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    let (channel_id, description) = {
        let Some(ask) = data.with_ok(|cfg| cfg.asks.get(&msg_id).cloned()).await? else {
            return Ok(());
        };
        if ask.description.is_some() {
            return Ok(());
        }
        let description = match &ask.url {
            Some(url) => {
                tracing::debug!("fetching game description from {}", url);
                let html = reqwest::get(url.as_str()).await?.text().await?;
                let document = scraper::Html::parse_document(&html);
                let selector = scraper::Selector::parse(".game_description_snippet").unwrap();
                document.select(&selector).next().map(|x| x.text().collect::<String>())
            }
            None => return Ok(()),
        };
        (ask.channel_id, description)
    };

    let Some(edit) = data
        .with_mut_ok(|cfg| {
            let ask = cfg.asks.get_mut(&msg_id)?;
            ask.description = description;
            Some(ask.edit_message())
        })
        .await?
    else {
        return Ok(());
    };

    edit.execute(ctx, (channel_id, msg_id, None)).await?;

    Ok(())
}

async fn update_ask_message(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    tracing::info!("Updating ask {msg_id}");

    let (channel_id, edit, ping) = data
        .with_mut(|cfg| {
            let ask = cfg.asks.get_mut(&msg_id).ok_or_eyre("Can't update missing ask")?;
            Ok((ask.channel_id, ask.edit_message(), ask.ping(msg_id)))
        })
        .await?;

    edit.execute(ctx, (channel_id, msg_id, None)).await?;

    if let Some(ping) = ping {
        ping.execute(ctx, (channel_id, None)).await?;
    };

    Ok(())
}

async fn disable_ask_message(
    ctx: &Context,
    data: &impl With<ConfigT>,
    msg_id: MessageId,
) -> Result<()> {
    tracing::info!("Disabling ask {msg_id}");

    let (channel_id, edit, embed) = data
        .with_mut(|cfg| {
            let ask = cfg.asks.remove(&msg_id).ok_or_eyre("Can't remove missing ask")?;
            Ok((ask.channel_id, ask.edit_message(), ask.embed()))
        })
        .await?;

    edit.embed(embed.colour(serenity::colours::branding::BLACK))
        .components(vec![])
        .execute(ctx, (channel_id, msg_id, None))
        .await?;

    Ok(())
}

pub enum JoinOrLeave {
    Join,
    Leave,
}

pub async fn button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
    join_or_leave: JoinOrLeave,
) -> Result<()> {
    let player_id = component.user.id;

    let success_response = |ask: &Ask| {
        CreateInteractionResponse::UpdateMessage(
            CreateInteractionResponseMessage::new()
                .embed(ask.embed())
                .components(vec![ask.action_row()]),
        )
    };

    let error_response = |content, components| {
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .ephemeral(true)
                .content(content)
                .components(components),
        )
    };

    let (channel_id, response, ping) = ctx
        .user_data
        .with_mut(|cfg| {
            let ask = cfg.asks.get_mut(&component.message.id).ok_or_eyre("unknown ask")?;
            let response = match join_or_leave {
                JoinOrLeave::Join => {
                    if ask.full() {
                        error_response("Sorry, the lobby is already full", vec![])
                    } else if ask.players.contains(&player_id) {
                        CreateInteractionResponse::Acknowledge
                    } else {
                        ask.players.push(player_id);
                        success_response(ask)
                    }
                }
                JoinOrLeave::Leave => {
                    if !ask.players.contains(&player_id) {
                        error_response(
                            "Press again to leave the server",
                            vec![CreateActionRow::Buttons(vec![
                                serenity::CreateButton::new(LEAVE_SERVER_BUTTON_ID)
                                    .label("LEAVE SERVER")
                                    .style(serenity::ButtonStyle::Danger),
                            ])],
                        )
                    } else {
                        ask.players.retain(|&x| x != player_id);
                        success_response(ask)
                    }
                }
            };
            Ok((ask.channel_id, response, ask.ping(component.message.id)))
        })
        .await?;

    component.create_response(ctx.serenity_context, response).await?;

    if let Some(ping) = ping {
        ping.execute(ctx.serenity_context, (channel_id, None)).await?;
    }

    Ok(())
}

pub async fn leave_server(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
) -> Result<()> {
    serenity::CreateQuickModal::new("You have been banned!")
        .field(
            serenity::CreateInputText::new(serenity::InputTextStyle::Short, "Ban Reason", "")
                .value("You pressed the button :("),
        )
        .timeout(Duration::from_secs(2 * 60))
        .execute(ctx.serenity_context, component.id, &component.token)
        .await?;
    Ok(())
}

/// Add /ask default values for matching game titles
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn new_ask_defaults<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Regex to match the game title"] title_pattern: String,
    #[description = "(Default) Minimum number of players"] min_players: Option<u32>,
    #[description = "(Default) Maximum number of players"] max_players: Option<u32>,
    #[description = "(Default) Link to the game"] url: Option<Url>,
    #[description = "(Default) Description of the game"] description: Option<String>,
    #[description = "(Default) Thumbnail of the game"] thumbnail_url: Option<String>,
) -> Result<()> {
    let title_pattern = Regex::new(&format!("(?i){title_pattern}")).wrap_err("Invalid regex")?;

    ctx.data()
        .with_mut_ok(|cfg| {
            cfg.defaults.insert(
                LiteralRegex(title_pattern),
                AskDefaults { min_players, max_players, url, description, thumbnail_url },
            );
        })
        .await?;

    ctx.say("📝 Ask defaults updated").await?;

    Ok(())
}

/// Delete /ask default values where a title is matched by the regex
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn delete_ask_defaults<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Regex to match the game title"] title_pattern: String,
) -> Result<()> {
    #[allow(clippy::mutable_key_type)]
    let deleted = ctx
        .data()
        .with_mut_ok(|cfg| {
            cfg.defaults
                .extract_if(|regex, _| regex.0.is_match(&title_pattern).is_ok_and(|x| x))
                .collect::<BTreeMap<_, _>>()
        })
        .await?;

    if deleted.is_empty() {
        ctx.say("❌ No matching ask defaults were found").await?;
    } else {
        let attachment = serenity::CreateAttachment::bytes(
            deleted.iter().map(|(re, v)| format!("{:?}: {v:?}", re.0.as_str())).join("\n"),
            "deleted.txt",
        );
        let reply =
            poise::CreateReply::new().content("🗑️ Ask defaults deleted").attachment(attachment);
        ctx.send(reply).await?;
    }
    Ok(())
}
