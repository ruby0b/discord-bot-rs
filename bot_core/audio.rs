use crate::State;
use eyre::{OptionExt as _, Result, WrapErr as _};
use poise::serenity_prelude::{ChannelId, Context, GuildId};
use songbird::input::{File, Input};
use songbird::{Call, Event, EventContext, Songbird, TrackEvent};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
pub struct StateT {
    lock: Mutex<()>,
}

#[derive(Clone, Debug)]
pub struct Playable(OwnedInput, Vec<OwnedInput>);

impl Playable {
    pub fn file(path: PathBuf) -> Self {
        Self(OwnedInput::File(path), vec![])
    }

    #[allow(dead_code)]
    pub fn bytes(bytes: &[u8]) -> Self {
        Self(OwnedInput::Bytes(bytes.to_vec()), vec![])
    }
}

impl std::ops::Add<Playable> for Playable {
    type Output = Playable;

    fn add(self, rhs: Playable) -> Self::Output {
        Playable(self.0, self.1.into_iter().chain(std::iter::once(rhs.0)).chain(rhs.1).collect())
    }
}

#[derive(Clone, Debug)]
enum OwnedInput {
    File(PathBuf),
    #[allow(dead_code)]
    Bytes(Vec<u8>),
}

impl From<OwnedInput> for Input {
    fn from(input: OwnedInput) -> Self {
        match input {
            OwnedInput::File(path) => File::new(path).into(),
            OwnedInput::Bytes(bytes) => bytes.into(),
        }
    }
}

pub async fn play(
    ctx: &Context,
    data: &impl State<StateT>,
    guild_id: GuildId,
    channel_id: ChannelId,
    sounds: Playable,
) -> Result<()> {
    match join_play_leave(ctx, data, guild_id, channel_id, sounds)
        .await
        .wrap_err(format!("Failed to play audio in {guild_id}/{channel_id}"))?
    {
        Busyness::Success => tracing::info!("Played audio in {}/{}", guild_id, channel_id),
        Busyness::Busy => {}
    };
    Ok(())
}

pub enum Busyness {
    Success,
    Busy,
}

pub async fn join_play_leave(
    ctx: &Context,
    data: &impl State<StateT>,
    guild_id: GuildId,
    channel_id: ChannelId,
    sounds: Playable,
) -> Result<Busyness> {
    let state = data.state();
    let Ok(_lock) = state.lock.try_lock() else { return Ok(Busyness::Busy) };

    let manager = songbird::get(ctx).await.ok_or_eyre("Songbird voice client not initialized")?;
    match manager.join(guild_id, channel_id).await {
        Ok(handler_lock) => {
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
            add_queue(&mut handler, guild_id, manager, sounds).await?;
            Ok(Busyness::Success)
        }
        Err(e) => Err(e)?,
    }
}

pub async fn add_queue(
    call: &mut Call,
    guild_id: GuildId,
    manager: Arc<Songbird>,
    Playable(sound, queue): Playable,
) -> Result<()> {
    let track_handle = call.play_only_input(sound.into());
    if let Some((next, queue)) = queue.split_first() {
        track_handle.add_event(
            Event::Track(TrackEvent::End),
            PlayNextAction { manager, guild_id, sounds: Playable(next.clone(), queue.to_vec()) },
        )?;
    } else {
        track_handle.add_event(Event::Track(TrackEvent::End), LeaveAction { manager, guild_id })?;
    }
    Ok(())
}

struct TrackErrorNotifier;

#[async_trait::async_trait]
impl songbird::EventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                let track = &handle.uuid();
                let status = &state.playing;
                tracing::error!("Track {track:?} encountered an error: {status:?}");
            }
        }
        None
    }
}

struct PlayNextAction {
    manager: Arc<Songbird>,
    guild_id: GuildId,
    sounds: Playable,
}

#[async_trait::async_trait]
impl songbird::EventHandler for PlayNextAction {
    async fn act(&self, _: &EventContext<'_>) -> Option<Event> {
        let handler_lock = self.manager.get(self.guild_id)?;
        let mut handler = handler_lock.lock().await;
        add_queue(&mut handler, self.guild_id, self.manager.clone(), self.sounds.clone())
            .await
            .ok()?;
        None
    }
}

struct LeaveAction {
    manager: Arc<Songbird>,
    guild_id: GuildId,
}

#[async_trait::async_trait]
impl songbird::EventHandler for LeaveAction {
    async fn act(&self, _: &EventContext<'_>) -> Option<Event> {
        let handler_lock = self.manager.get(self.guild_id)?;
        let mut handler = handler_lock.lock().await;
        handler.leave().await.ok()?;
        None
    }
}
