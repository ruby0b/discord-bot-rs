#![feature(trait_alias)]

pub mod audio;
pub mod autocomplete;
pub mod hash_store;
pub mod serde;
pub mod template;

use eyre::{OptionExt as _, Result};
use poise::serenity_prelude::{ChannelId, Member, VoiceState};
use std::sync::Arc;

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

pub fn avatar_url(member: Member) -> String {
    member.avatar_url().or(member.user.avatar_url()).unwrap_or(member.user.default_avatar_url())
}
