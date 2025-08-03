#![feature(btree_extract_if)]
#![allow(clippy::mutable_key_type)]

mod ask;
mod buttons;
mod cmd_ask;
mod cmd_delete_ask_defaults;
mod cmd_new_ask_defaults;
mod schedule_ask_updates;

use crate::ask::Ask;
pub use crate::buttons::*;
pub use crate::cmd_ask::*;
pub use crate::cmd_delete_ask_defaults::*;
pub use crate::cmd_new_ask_defaults::*;
use crate::schedule_ask_updates::schedule_ask_updates;
use bot_core::With;
use bot_core::serde::LiteralRegex;
use chrono::TimeDelta;
use eyre::Result;
use poise::serenity_prelude::{Context, MessageId};
use std::collections::BTreeMap;
use url::Url;

pub const JOIN_BUTTON_ID: &str = "ask.join_button";
pub const LEAVE_BUTTON_ID: &str = "ask.leave_button";
pub const DECLINE_BUTTON_ID: &str = "ask.decline_button";
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

pub async fn load_asks(ctx: &Context, data: &impl With<ConfigT>) -> Result<()> {
    tracing::debug!("Loading asks from config");
    let ask_config = data.with_ok(|cfg| cfg.clone()).await?;
    for (msg_id, ask) in ask_config.asks {
        schedule_ask_updates(ctx, data, &ask, msg_id, ask_config.expiration).await;
    }
    Ok(())
}
