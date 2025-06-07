#![feature(trait_alias)]
#![allow(clippy::mutable_key_type)]

mod tts;

use bot_core::audio::{Playable, play};
use bot_core::serde::LiteralRegex;
use bot_core::{EvtContext, OptionExt as _, State, VoiceChange, With, hash_store, template};
use dashmap::DashMap;
use eyre::{OptionExt as _, Result, bail};
use itertools::Itertools;
use poise::serenity_prelude::{CacheHttp, ChannelId, GuildId, Presence, UserId, VoiceState};
use rand::seq::IteratorRandom as _;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigT {
    clips: BTreeMap<String, bot_core::serde::MessageLink>,
    join: Tts,
    leave: Tts,
    activities: BTreeMap<LiteralRegex, ActivityTts>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
struct Tts {
    chance: f64,
    #[serde(with = "bot_core::serde::duration_seconds")]
    cooldown: Duration,
    messages: Vec<String>,
    user_messages: BTreeMap<UserId, Vec<String>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
struct ActivityTts {
    details: Option<LiteralRegex>,
    state: Option<LiteralRegex>,
    #[serde(flatten)]
    config: Tts,
}

#[derive(Default)]
pub struct StateT {
    cooldowns: DashMap<CooldownId, Mutex<Instant>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CooldownId {
    Join,
    Leave,
    Activity(String),
}

pub trait UserDataT = With<ConfigT> + State<StateT> + State<bot_core::audio::StateT>;

pub async fn voice_update(
    ctx: EvtContext<'_, impl UserDataT>,
    guild_id: GuildId,
    (old, new): (&Option<VoiceState>, &VoiceState),
) -> Result<()> {
    let (get, channel_id, cooldown_id) = match VoiceChange::new((old, new)) {
        VoiceChange::Join { to } => ((|c| &c.join) as fn(&ConfigT) -> &Tts, to, CooldownId::Join),
        VoiceChange::Leave { from } => {
            ((|c| &c.leave) as fn(&ConfigT) -> &Tts, from, CooldownId::Leave)
        }
        _ => return Ok(()),
    };

    if new.user_id.to_user(ctx.serenity_context).await.is_ok_and(|user| user.bot) {
        return Ok(());
    }

    let config = ctx.user_data.with_ok(|cfg| get(cfg).clone()).await?;

    play_tts(&ctx, guild_id, channel_id, new.user_id, &config, cooldown_id).await?;

    Ok(())
}

pub async fn presence_update(
    ctx: EvtContext<'_, impl UserDataT>,
    presence: &Presence,
) -> Result<()> {
    let guild_id = presence.guild_id.ok_or_eyre("No guild ID")?;
    let vc_id = {
        let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
        let Some(voice_state) = guild.voice_states.get(&presence.user.id) else { return Ok(()) };
        let Some(vc_id) = voice_state.channel_id else { return Ok(()) };
        vc_id
    };

    let games = ctx.user_data.with_ok(|cfg: &ConfigT| cfg.activities.clone()).await?;

    for (activity, (name, game)) in presence.activities.iter().cartesian_product(games.iter()) {
        if !name.0.is_match(&activity.name)? {
            continue;
        }

        if let Some(r) = game.details.as_ref()
            && !r.0.is_match(activity.details.as_deref().unwrap_or_default())?
        {
            continue;
        }

        if let Some(r) = game.state.as_ref()
            && !r.0.is_match(activity.state.as_deref().unwrap_or_default())?
        {
            continue;
        }

        let cooldown_id = CooldownId::Activity(name.0.as_str().to_string());
        if play_tts(&ctx, guild_id, vc_id, presence.user.id, &game.config, cooldown_id).await? {
            break;
        }
    }

    Ok(())
}

async fn play_tts(
    ctx: &EvtContext<'_, impl UserDataT>,
    guild_id: GuildId,
    channel_id: ChannelId,
    user_id: UserId,
    config: &Tts,
    cooldown_id: CooldownId,
) -> Result<bool> {
    let new_time = || Instant::now().checked_add(config.cooldown).ok_or_eyre("Instant overflow");
    let state: Arc<StateT> = ctx.user_data.state();
    match state.cooldowns.entry(cooldown_id.clone()) {
        dashmap::Entry::Occupied(entry) => {
            let mut timer = entry.get().lock().await;
            if timer.elapsed().is_zero() {
                return Ok(false);
            }
            *timer = new_time()?;
        }
        dashmap::Entry::Vacant(entry) => {
            entry.insert(Mutex::new(new_time()?));
        }
    }

    tracing::debug!("Want to play TTS {cooldown_id:?}");

    if rand::random::<f64>() >= config.chance {
        return Ok(false);
    }

    let member = guild_id.member(ctx.serenity_context, user_id).await?;
    let mut vars = HashMap::new();
    vars.insert("name".to_string(), member.display_name().to_string());

    let clips = get_clips(&ctx.serenity_context, ctx.user_data).await?;

    let Some(template) = config
        .user_messages
        .get(&user_id)
        .and_then(|v| v.iter().choose(&mut rand::rng()))
        .or_else(|| config.messages.iter().choose(&mut rand::rng()))
    else {
        return Ok(false);
    };

    tracing::debug!("Playing TTS {cooldown_id:?} - {template}");

    let audio = template_to_audio(template, &vars, &clips).await?;
    play(ctx.serenity_context, ctx.user_data, guild_id, channel_id, audio).await?;
    Ok(true)
}

async fn template_to_audio(
    template: &str,
    vars: &HashMap<String, String>,
    clips: &HashMap<String, Playable>,
) -> Result<Playable> {
    let mut audio = None;
    let mut text_buffer = String::new();

    for chunk in template::template_to_chunks(template) {
        match chunk {
            template::Chunk::Text(text) => {
                text_buffer.push_str(&text);
            }
            template::Chunk::Variable(var) => {
                if let Some(text) = vars.get(&var) {
                    text_buffer.push_str(text);
                } else if let Some(clip) = clips.get(&var) {
                    if !text_buffer.is_empty() {
                        extend_audio(&mut audio, tts::get_tts(&text_buffer).await?);
                        text_buffer.clear();
                    }
                    extend_audio(&mut audio, clip.clone());
                } else {
                    tracing::warn!("Unknown variable or file {{{var}}} in TTS template");
                }
            }
        }
    }

    if !text_buffer.is_empty() {
        extend_audio(&mut audio, tts::get_tts(&text_buffer).await?);
    }

    audio.ok_or_eyre("Empty audio")
}

fn extend_audio(audio_1: &mut Option<Playable>, audio_2: Playable) {
    *audio_1 = match audio_1.take() {
        Some(audio) => Some(audio + audio_2),
        None => Some(audio_2),
    }
}

pub async fn get_clips(
    chttp: impl CacheHttp,
    data: &impl With<ConfigT>,
) -> Result<HashMap<String, Playable>> {
    let files = data.with_ok(|cfg| cfg.clips.clone()).await?;

    let mut clips = HashMap::new();
    for (name, link) in files {
        let path = hash_store::get_or_store(
            link.to_string().as_bytes(),
            Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("mp3"),
            async {
                let message = link.channel_id.message(&chttp, link.message_id).await?;
                let Some(attachment) = message.attachments.first() else {
                    bail!("No attachment found in message {link}")
                };
                tracing::info!("Downloading audio clip {name}");
                Ok(attachment.download().await?)
            },
        )
        .await?;
        clips.insert(name, Playable::file(path));
    }

    Ok(clips)
}
