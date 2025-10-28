#![feature(btree_extract_if)]
#![allow(clippy::mutable_key_type)]

mod ask;
mod buttons;
mod cmd_ask;
mod cmd_delete_ask_defaults;
mod cmd_new_ask_defaults;
mod schedule_updates;
mod update_worker;

use crate::ask::Ask;
pub use crate::buttons::*;
pub use crate::cmd_ask::*;
pub use crate::cmd_delete_ask_defaults::*;
pub use crate::cmd_new_ask_defaults::*;
use crate::schedule_updates::schedule_ask_updates;
use crate::update_worker::{UpdateCommand, ask_update_worker};
use bot_core::serde::LiteralRegex;
use bot_core::{State, With};
use chrono::TimeDelta;
use eyre::Result;
use poise::serenity_prelude::{Context, MessageId};
use std::collections::BTreeMap;
use tokio::sync::{OnceCell, mpsc};
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

#[derive(Default)]
pub struct StateT {
    update_sender: OnceCell<mpsc::Sender<UpdateCommand>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
struct AskDefaults {
    min_players: Option<u32>,
    max_players: Option<u32>,
    url: Option<Url>,
    description: Option<String>,
    thumbnail_url: Option<String>,
}

pub async fn setup(ctx: Context, data: impl With<ConfigT> + State<StateT>) -> Result<()> {
    {
        tracing::debug!("Spawning ask update worker");
        let (tx, rx) = mpsc::channel::<UpdateCommand>(100);
        data.state().update_sender.set(tx)?;
        tokio::spawn(ask_update_worker(ctx, data.clone(), rx));
    }
    {
        tracing::debug!("Loading asks from config");
        let ask_config = data.with_ok(|cfg| cfg.clone()).await?;
        for (msg_id, ask) in ask_config.asks {
            schedule_ask_updates(&data, &ask, msg_id, ask_config.expiration).await;
        }
    }
    Ok(())
}
