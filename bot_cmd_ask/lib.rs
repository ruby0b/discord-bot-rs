#![allow(clippy::mutable_key_type)]

mod ask;
mod autocomplete;
mod buttons;
mod cmd_ask;
mod cmd_configure_ask_game;
mod cmd_delete_ask_game;
mod schedule_updates;
mod worker_ask_update;
mod worker_game_roles;

use crate::ask::Ask;
pub use crate::buttons::*;
pub use crate::cmd_ask::*;
pub use crate::cmd_configure_ask_game::*;
pub use crate::cmd_delete_ask_game::*;
use crate::schedule_updates::schedule_ask_updates;
use bot_core::serde::LiteralRegex;
use bot_core::{State, With};
use chrono::TimeDelta;
use eyre::Result;
use poise::serenity_prelude::{Context, GuildId, MessageId, RoleId, UserId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::{OnceCell, mpsc};
use url::Url;

pub const JOIN_BUTTON_ID: &str = "ask.join_button";
pub const LEAVE_BUTTON_ID: &str = "ask.leave_button";
pub const DECLINE_BUTTON_ID: &str = "ask.decline_button";
pub const LEAVE_SERVER_BUTTON_ID: &str = "ask.leave_server";
pub const TOGGLE_GAME_ROLE_BUTTON_ID: &str = "ask.toggle_game_role";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    #[serde(with = "bot_core::serde::td_seconds")]
    #[default(TimeDelta::hours(3))]
    expiration: TimeDelta,
    games: BTreeMap<String, Game>,
    asks: BTreeMap<MessageId, Ask>,
}

#[derive(Default)]
pub struct StateT {
    ask_update_sender: OnceCell<mpsc::Sender<worker_ask_update::Command>>,
    game_role_sender: OnceCell<mpsc::Sender<worker_game_roles::Command>>,
    serpapi_token: OnceCell<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Game {
    parent_role: String,
    title_pattern: LiteralRegex,
    defaults: GameDefaults,
    opted_out_users: BTreeSet<UserId>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
struct GameDefaults {
    min_players: Option<u32>,
    max_players: Option<u32>,
    url: Option<Url>,
    description: Option<String>,
    thumbnail_url: Option<String>,
}

pub async fn setup(
    ctx: Context,
    data: impl With<ConfigT> + State<StateT> + State<GuildId>,
    serpapi_token: String,
) -> Result<()> {
    let state: Arc<StateT> = data.state();
    {
        state.serpapi_token.set(serpapi_token)?;
    }
    {
        tracing::debug!("Spawning ask update worker");
        let (tx, rx) = mpsc::channel::<worker_ask_update::Command>(100);
        state.ask_update_sender.set(tx)?;
        tokio::spawn(worker_ask_update::work(ctx.clone(), data.clone(), rx));
    }
    {
        tracing::debug!("Spawning game role worker");
        let (tx, rx) = mpsc::channel::<worker_game_roles::Command>(100);
        state.game_role_sender.set(tx)?;
        tokio::spawn(worker_game_roles::work(ctx, data.clone(), rx));
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
