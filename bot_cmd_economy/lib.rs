mod account;
mod table;

pub use crate::account::*;
pub use crate::table::*;
use bot_core::With;
use chrono::{DateTime, TimeDelta, Utc};
use dashmap::DashMap;
use eyre::Result;
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, Mentionable as _, UserId,
};
use serde_with::{DisplayFromStr, serde_as};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

pub const BUYIN_BUTTON_ID: &str = "economy.buyin";
pub const PAYOUT_BUTTON_ID: &str = "economy.payout";

#[serde_as]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    currency: String,
    #[serde(with = "bot_core::serde::td_seconds")]
    #[default(TimeDelta::days(1))]
    income_cooldown: TimeDelta,
    #[default(100)]
    income_amount: u64,
    account: BTreeMap<UserId, UserAccount>,
    #[serde_as(as = "BTreeMap<DisplayFromStr, _>")]
    gambling_tables: BTreeMap<Uuid, GamblingTable>,
}

#[derive(Default)]
pub struct StateT {
    table_locks: DashMap<Uuid, Arc<Mutex<()>>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
struct UserAccount {
    balance: u64,
    last_income: Option<DateTime<Utc>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
struct GamblingTable {
    dealer: UserId,
    name: String,
    buyin: u64,
    players: BTreeMap<UserId, u64>,
    pot: u64,
}

impl GamblingTable {
    fn embed(&self, cur: &str) -> CreateEmbed {
        let mut embed = CreateEmbed::default()
            .title(self.name.clone())
            .colour(Colour::GOLD)
            .field("Buy-in", currency(cur, self.buyin), true)
            .field("Pot", currency(cur, self.pot), true);

        if !self.players.is_empty() {
            embed = embed.field(
                "Bets",
                self.players
                    .iter()
                    .map(|(p, &bet)| format!("{}: {}", p.mention(), currency(cur, bet)))
                    .collect::<Vec<_>>()
                    .join("\n"),
                false,
            );
        }

        embed
    }

    fn action_row(&self, id: Uuid) -> CreateActionRow {
        let buyin_button = CreateButton::new(format!("{BUYIN_BUTTON_ID}:{id}"))
            .style(ButtonStyle::Success)
            .label("Buy In");
        let payout_button = CreateButton::new(format!("{PAYOUT_BUTTON_ID}:{id}"))
            .style(ButtonStyle::Primary)
            .label("Pay Out");
        CreateActionRow::Buttons(vec![buyin_button, payout_button])
    }

    fn reply(&self, cur: &str, id: Uuid) -> CreateReply {
        CreateReply::new().embed(self.embed(cur)).components(vec![self.action_row(id)])
    }

    fn deactivated_reply(&self, cur: &str) -> CreateReply {
        CreateReply::new().embed(self.embed(cur).colour(Colour::DARKER_GREY)).components(vec![])
    }
}

fn currency(symbol: &str, money: u64) -> String {
    format!("{money} {symbol}")
}

async fn get_currency(data: &impl With<ConfigT>) -> Result<String> {
    data.with_ok(|cfg| cfg.currency.clone()).await
}
