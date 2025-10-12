use bot_core::With;
use derive_more::{AsMut, AsRef};
use eyre::Result;
use poise::serenity_prelude::GuildId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Default, AsRef)]
pub struct GuildData(
    // TODO: multiple guild support
    Arc<GuildId>,
    Arc<crate::config::GuildConfig<GuildConfigT>>,
    Arc<bot_core::audio::StateT>,
    Arc<bot_cmd_tts::StateT>,
    Arc<bot_cmd_ephemeral_voice_channels::StateT>,
    Arc<bot_cmd_periodic_region_change::StateT>,
    Arc<bot_cmd_economy::StateT>,
);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default, AsRef, AsMut)]
pub struct GuildConfigT {
    #[serde(default)]
    ask: bot_cmd_ask::ConfigT,
    #[serde(default)]
    bedtime: bot_cmd_bedtime::ConfigT,
    #[serde(default)]
    role_icon_change: bot_cmd_role_icon::ConfigT,
    #[serde(default)]
    ephemeral_voice_channels: bot_cmd_ephemeral_voice_channels::ConfigT,
    #[serde(default)]
    periodic_region_change: bot_cmd_periodic_region_change::ConfigT,
    #[serde(default)]
    roles: bot_cmd_roles::ConfigT,
    #[serde(default)]
    tts: bot_cmd_tts::ConfigT,
    #[serde(default)]
    economy: bot_cmd_economy::ConfigT,
}

#[async_trait::async_trait]
impl<ConfigT> With<ConfigT> for GuildData
where
    GuildConfigT: AsRef<ConfigT> + AsMut<ConfigT>,
    ConfigT: Clone,
{
    async fn with<Output>(
        &self,
        f: impl Send + for<'a> FnOnce(&'a ConfigT) -> Result<Output>,
    ) -> Result<Output> {
        let config: &Arc<crate::config::GuildConfig<_>> = self.as_ref();
        config.with(|cfg| f(cfg.as_ref())).await
    }
    async fn with_mut<Output>(
        &self,
        f: impl Send + for<'a> FnOnce(&'a mut ConfigT) -> Result<Output>,
    ) -> Result<Output> {
        let config: &Arc<crate::config::GuildConfig<_>> = self.as_ref();
        config.with_mut(|cfg| f(cfg.as_mut())).await
    }
}
