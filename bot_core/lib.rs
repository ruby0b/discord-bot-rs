#![feature(trait_alias)]

pub mod audio;
pub mod autocomplete;
pub mod choice_parameters;
pub mod color_parameter;
pub mod ext {
    pub mod create_reply;
    pub mod option;
    pub mod result;
    pub mod set;
}
pub mod hash_store;
pub mod interval_set;
pub mod iso_weekday;
pub mod lock_set;
pub mod serde;
pub mod template;
pub mod timer_queue;
pub mod voice_change;

use crate::ext::option::OptionExt as _;
use chrono::{DateTime, Local, NaiveDateTime, NaiveTime, TimeZone as _, Utc};
use eyre::Result;
use poise::serenity_prelude::{
    Builder as _, Cache, Context, CreateInteractionResponse, GuildId, Member, ModalInteraction, UserId,
};
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
    async fn with<Output>(&self, f: impl Send + for<'a> FnOnce(&'a Config) -> Result<Output>) -> Result<Output>;
    async fn with_mut<Output>(&self, f: impl Send + for<'a> FnOnce(&'a mut Config) -> Result<Output>)
    -> Result<Output>;
    async fn with_ok<Output>(&self, f: impl Send + for<'a> FnOnce(&'a Config) -> Output) -> Result<Output> {
        self.with(|cfg| Ok(f(cfg))).await
    }
    async fn with_mut_ok<Output>(&self, f: impl Send + for<'a> FnOnce(&'a mut Config) -> Output) -> Result<Output> {
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

/// Returns a function x -> (x, f(x))
pub fn to_snd<K, V>(f: impl Fn(&K) -> V) -> impl Fn(K) -> (K, V) {
    move |key: K| {
        let value = f(&key);
        (key, value)
    }
}

pub fn avatar_url(member: &Member) -> String {
    member.avatar_url().or(member.user.avatar_url()).unwrap_or(member.user.default_avatar_url())
}

pub async fn deferred_message(ctx: &Context, interaction: &ModalInteraction) -> Result<()> {
    CreateInteractionResponse::Defer(Default::default()).execute(ctx, (interaction.id, &interaction.token)).await?;
    Ok(())
}

pub fn safe_name(ctx: &impl AsRef<Cache>, user_id: UserId) -> String {
    user_id.to_user_cached(&ctx).map_or(user_id.to_string(), |u| u.display_name().to_string())
}

pub fn get_member(ctx: &impl AsRef<Cache>, guild_id: GuildId, user_id: UserId) -> Option<Member> {
    let guild = ctx.as_ref().guild(guild_id).inspect_none(|| tracing::warn!("Guild not in cache: {guild_id}"))?;
    guild.members.get(&user_id).inspect_none(|| tracing::warn!("Member not found in cache: {guild_id}")).cloned()
}

pub fn naive_time_to_next_datetime(naive_time: NaiveTime) -> Option<DateTime<Local>> {
    let now = Utc::now().naive_local();
    let date = if naive_time > now.time() { now.date() } else { now.date().succ_opt().unwrap() };
    Local.from_local_datetime(&NaiveDateTime::new(date, naive_time)).single()
}
