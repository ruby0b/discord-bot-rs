mod account;
mod buy_in;
mod gamble;
mod pay_out;

pub use crate::account::*;
pub use crate::buy_in::*;
pub use crate::gamble::*;
pub use crate::pay_out::*;
use bot_core::{LockSet, With};
use chrono::{DateTime, Utc};
use eyre::Result;
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, Colour, CreateActionRow, CreateButton, CreateEmbed, Mentionable as _, UserId,
};
use serde_with::{DisplayFromStr, serde_as};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const BUYIN_BUTTON_ID: &str = "economy.buyin";
pub const PAY_TABLE_BUTTON_ID: &str = "economy.pay_table";
pub const PAY_PLAYER_BUTTON_ID: &str = "economy.pay_player";

#[serde_as]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    currency: Currency,
    income: Income,
    account: BTreeMap<UserId, UserAccount>,
    #[serde_as(as = "BTreeMap<DisplayFromStr, _>")]
    gambling_tables: BTreeMap<Uuid, GamblingTable>,
}

#[derive(Default)]
pub struct StateT {
    table_locks: LockSet<Uuid>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
struct Currency {
    symbol: String,
}

impl Currency {
    async fn read(data: &impl With<ConfigT>) -> Result<Currency> {
        data.with_ok(|cfg| cfg.currency.clone()).await
    }

    fn fmt(&self, money: u64) -> String {
        format!("{money} {}", self.symbol)
    }
}

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    Copy,
)]
struct Income {
    daily: u64,
    weekly: u64,
    monthly: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
struct UserAccount {
    balance: u64,
    last_claim: Option<DateTime<Utc>>,
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
    fn embed(&self, cur: &Currency) -> CreateEmbed {
        let mut embed = CreateEmbed::default()
            .title(self.name.clone())
            .colour(Colour::GOLD)
            .field("Buy-in", cur.fmt(self.buyin), true)
            .field("Pot", cur.fmt(self.pot), true);

        if !self.players.is_empty() {
            embed = embed.field(
                "Bets",
                self.players
                    .iter()
                    .map(|(p, &bet)| format!("{}: {}", p.mention(), cur.fmt(bet)))
                    .collect::<Vec<_>>()
                    .join("\n"),
                false,
            );
        }

        embed
    }

    fn components(&self, id: Uuid) -> Vec<CreateActionRow> {
        let buyin_button = CreateButton::new(format!("{BUYIN_BUTTON_ID}:{id}"))
            .style(ButtonStyle::Success)
            .label("Buy In")
            .emoji('💸');
        let payout_button = CreateButton::new(format!("{PAY_TABLE_BUTTON_ID}:{id}"))
            .style(ButtonStyle::Secondary)
            .label("Pay Everyone")
            .emoji('💰');
        let payout_player_button = CreateButton::new(format!("{PAY_PLAYER_BUTTON_ID}:{id}"))
            .style(ButtonStyle::Secondary)
            .label("Pay Player")
            .emoji('🪙');
        vec![CreateActionRow::Buttons(vec![buyin_button, payout_button, payout_player_button])]
    }

    fn reply(&self, cur: &Currency, id: Uuid) -> CreateReply {
        CreateReply::new().embed(self.embed(cur)).components(self.components(id))
    }

    fn deactivated_reply(&self, cur: &Currency) -> CreateReply {
        CreateReply::new().embed(self.embed(cur).colour(Colour::DARKER_GREY)).components(vec![])
    }
}
