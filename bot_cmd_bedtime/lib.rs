mod bedtime;
mod buttons;
mod cmd;
mod r#loop;

use crate::bedtime::Bedtime;
pub use crate::buttons::*;
pub use crate::cmd::*;
use crate::r#loop::bedtime_loop;
use bot_core::serde::LiteralRegex;
use bot_core::{State, With};
use chrono::TimeDelta;
use eyre::Result;
use poise::serenity_prelude::RoleId;
use poise::serenity_prelude::all::GuildId;
use poise::serenity_prelude::prelude::Context;
use std::collections::BTreeMap;
use uuid::Uuid;

pub const TOGGLE_WEEKDAY_BUTTON_ID: &str = "bedtime.weekday";
pub const DELETE_BUTTON_ID: &str = "bedtime.delete";
pub const SELECT_BEDTIME_ID: &str = "bedtime.select";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    #[serde(with = "bot_core::serde::td_seconds")]
    #[default(TimeDelta::hours(6))]
    duration: TimeDelta,
    ignored_vc_description: Option<LiteralRegex>,
    role: Option<RoleId>,
    bedtimes: BTreeMap<Uuid, Bedtime>,
}

pub async fn setup(ctx: Context, data: impl With<ConfigT> + State<GuildId>) -> Result<()> {
    tokio::spawn(bedtime_loop(ctx, data));
    Ok(())
}
