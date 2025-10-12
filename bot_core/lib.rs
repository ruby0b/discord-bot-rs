#![feature(trait_alias)]

pub mod audio;
pub mod autocomplete;
pub mod choice_parameters;
pub mod color_parameter;
pub mod hash_store;
pub mod result_ext;
pub mod serde;
pub mod template;
pub mod timer_queue;

use dashmap::DashMap;
use eyre::{OptionExt as _, Result};
use poise::CreateReply;
use poise::serenity_prelude::{
    Builder as _, Cache, ChannelId, ComponentInteraction, Context, CreateInteractionResponse,
    Member, Message, ModalInteraction, UserId, VoiceState,
};
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type EvtContext<'a, D> = poise::FrameworkContext<'a, D, eyre::Error>;
pub type CmdContext<'a, D> = poise::Context<'a, D, eyre::Error>;

pub trait UserData = Send + Sync + Clone + 'static;

/// Access RwLock-protected stuff generically using closures.
#[async_trait::async_trait]
pub trait With<Config>
where
    Self: UserData,
{
    async fn with<Output>(
        &self,
        f: impl Send + for<'a> FnOnce(&'a Config) -> Result<Output>,
    ) -> Result<Output>;
    async fn with_mut<Output>(
        &self,
        f: impl Send + for<'a> FnOnce(&'a mut Config) -> Result<Output>,
    ) -> Result<Output>;
    async fn with_ok<Output>(
        &self,
        f: impl Send + for<'a> FnOnce(&'a Config) -> Output,
    ) -> Result<Output> {
        self.with(|cfg| Ok(f(cfg))).await
    }
    async fn with_mut_ok<Output>(
        &self,
        f: impl Send + for<'a> FnOnce(&'a mut Config) -> Output,
    ) -> Result<Output> {
        self.with_mut(|cfg| Ok(f(cfg))).await
    }
}

/// Has a canonical implementation, just implement/derive `AsRef<Arc<Data>>` and `Clone`.
pub trait State<Data>
where
    Self: AsRef<Arc<Data>> + UserData,
{
    fn state(&self) -> Arc<Data> {
        self.as_ref().clone()
    }
}

impl<T, Data> State<Data> for T where T: AsRef<Arc<Data>> + UserData {}

/// Extension trait for `Option<T>`
pub trait OptionExt<T> {
    /// Convert None to an error
    fn some(self) -> Result<T>;
}

impl<T> OptionExt<T> for Option<T> {
    fn some(self) -> Result<T> {
        self.ok_or_eyre("Expected Some but got None")
    }
}

/// A user's voice state change in a guild regarding their current voice channel.
#[derive(Debug, PartialEq, Eq)]
pub enum VoiceChange {
    Join { to: ChannelId },
    Leave { from: ChannelId },
    Move { from: ChannelId, to: ChannelId },
    Stay,
}

impl VoiceChange {
    pub fn new((old, new): (&Option<VoiceState>, &VoiceState)) -> VoiceChange {
        let old_channel_id = old.as_ref().and_then(|old| old.channel_id);
        match (old_channel_id, new.channel_id) {
            (None, Some(to)) => VoiceChange::Join { to },
            (Some(from), None) => VoiceChange::Leave { from },
            (Some(from), Some(to)) if from != to => VoiceChange::Move { from, to },
            _ => VoiceChange::Stay,
        }
    }
}

pub fn avatar_url(member: &Member) -> String {
    member.avatar_url().or(member.user.avatar_url()).unwrap_or(member.user.default_avatar_url())
}

pub async fn deferred_message(ctx: &Context, interaction: &ModalInteraction) -> Result<()> {
    CreateInteractionResponse::Defer(Default::default())
        .execute(ctx, (interaction.id, &interaction.token))
        .await?;
    Ok(())
}

pub fn safe_name(ctx: &impl AsRef<Cache>, user_id: &UserId) -> String {
    user_id.to_user_cached(&ctx).map_or(user_id.to_string(), |u| u.display_name().to_string())
}

// todo generalize ComponentInteraction and ModalInteraction
#[async_trait::async_trait]
pub trait CreateReplyExt {
    async fn respond_to_interaction(
        self,
        ctx: &Context,
        interaction: &ComponentInteraction,
    ) -> Result<()>;

    async fn edit_interaction(
        self,
        ctx: &Context,
        interaction: &ModalInteraction,
    ) -> Result<Message>;

    async fn edit_message(self, ctx: &Context, message: &Message) -> Result<Message>;
}

#[async_trait::async_trait]
impl CreateReplyExt for CreateReply {
    async fn respond_to_interaction(
        self,
        ctx: &Context,
        interaction: &ComponentInteraction,
    ) -> Result<()> {
        Ok(CreateInteractionResponse::Message(self.to_slash_initial_response(Default::default()))
            .execute(ctx, (interaction.id, &interaction.token))
            .await?)
    }

    async fn edit_interaction(
        self,
        ctx: &Context,
        interaction: &ModalInteraction,
    ) -> Result<Message> {
        Ok(self
            .to_slash_initial_response_edit(Default::default())
            .execute(ctx, &interaction.token)
            .await?)
    }

    async fn edit_message(self, ctx: &Context, message: &Message) -> Result<Message> {
        Ok(self
            .to_prefix_edit(Default::default())
            .execute(ctx, (message.channel_id, message.id, Some(message.author.id)))
            .await?)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LockSet<K: Eq + Hash>(DashMap<K, Arc<Mutex<()>>>);

impl<K: Eq + Hash> LockSet<K> {
    pub fn get(&self, key: K) -> Arc<Mutex<()>> {
        self.0.entry(key).or_default().clone()
    }
}

/// Returns a function x -> (x, f(x))
pub fn to_snd<K, V>(f: impl Fn(&K) -> V) -> impl Fn(K) -> (K, V) {
    move |key: K| {
        let value = f(&key);
        (key, value)
    }
}
