use bot_core::{CmdContext, EvtContext, OptionExt as _, With};
use chrono::{DateTime, Duration, Utc};
use eyre::{OptionExt, Result, ensure};
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed,
    CreateInteractionResponse, Mentionable as _, UserId,
};
use serde_with::{DisplayFromStr, serde_as};
use std::collections::BTreeMap;
use uuid::Uuid;

pub const BUYIN_BUTTON_ID: &str = "economy.buyin";
pub const PAYOUT_BUTTON_ID: &str = "economy.payout";

#[serde_as]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    currency: String,
    #[default(Duration::days(1))]
    income_cooldown: Duration,
    #[default(100)]
    income_amount: u64,
    account: BTreeMap<UserId, UserAccount>,
    #[serde_as(as = "BTreeMap<_, BTreeMap<DisplayFromStr, _>>")]
    gambling_tables: BTreeMap<UserId, BTreeMap<Uuid, GamblingTable>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
struct UserAccount {
    balance: u64,
    last_income: Option<DateTime<Utc>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
struct GamblingTable {
    name: String,
    buyin: u64,
    players: BTreeMap<UserId, u64>,
    pot: u64,
}

impl GamblingTable {
    fn embed(&self, cur: &str) -> CreateEmbed {
        let mut embed = CreateEmbed::default()
            .title(self.name.clone())
            .field("Pot", currency(cur, self.pot), true)
            .field("Buy-in", currency(cur, self.buyin), true);

        if !self.players.is_empty() {
            embed = embed.field(
                "Players",
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

    fn create_reply(&self, cur: &str, id: Uuid) -> CreateReply {
        CreateReply::new().embed(self.embed(cur)).components(vec![self.action_row(id)])
    }
}

/// Check your balance and claim your income
#[poise::command(slash_command, guild_only)]
pub async fn balance<D: With<ConfigT>>(ctx: CmdContext<'_, D>) -> Result<()> {
    let cur = &get_currency(ctx.data()).await?;
    let (account, income) = ctx
        .data()
        .with_mut_ok(|cfg| {
            let account = cfg.account.entry(ctx.author().id).or_default();
            let mut income = None;
            if account.last_income.is_none_or(|date| date < Utc::now() - cfg.income_cooldown) {
                income = Some(cfg.income_amount);
                account.last_income = Some(Utc::now());
                account.balance += cfg.income_amount;
            }
            (account.clone(), income)
        })
        .await?;

    ctx.say(format!(
        "Your balance is: {}{}",
        currency(cur, account.balance),
        income.map(|i| format!(" (received {i} as income)")).unwrap_or_default()
    ))
    .await?;

    Ok(())
}

/// Create a new gambling table or display an existing one
#[poise::command(slash_command, guild_only)]
pub async fn gamble<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Buy-in amount for the table"] buyin: u64,
    #[description = "Name of the gambling table"] name: Option<String>,
) -> Result<()> {
    let cur = &get_currency(ctx.data()).await?;

    let id = Uuid::new_v4();
    let table = GamblingTable {
        name: format!(
            "{}'s {}{}Gambling Table",
            ctx.author_member().await.some()?.display_name(),
            name.clone().unwrap_or_default(),
            if name.is_some() { " " } else { "" },
        ),
        buyin,
        players: BTreeMap::new(),
        pot: 0,
    };

    let reply = table.create_reply(cur, id);

    ctx.data()
        .with_mut_ok(|cfg| {
            cfg.gambling_tables.entry(ctx.author().id).or_default().insert(id, table)
        })
        .await?;

    ctx.send(reply).await?;

    Ok(())
}

/// Display an existing gambling table
#[poise::command(slash_command, guild_only)]
pub async fn show_gamble<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    // todo: autocomplete
    #[description = "ID of the gambling table to show"] table_id: Uuid,
) -> Result<()> {
    let cur = &get_currency(ctx.data()).await?;

    let table = ctx
        .data()
        .with(|cfg| {
            let user_id = ctx.author().id;
            cfg.gambling_tables
                .get(&user_id)
                .and_then(|tables| tables.get(&table_id))
                .cloned()
                .ok_or_eyre("No gambling table found with id {table_id}")
        })
        .await?;

    ctx.send(table.create_reply(cur, table_id)).await?;

    Ok(())
}

pub async fn buyin_button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let user_id = component.user.id;
    let cur = &get_currency(ctx.user_data).await?;
    let table_id = Uuid::try_parse(param)?;

    let table = ctx
        .user_data
        .with_mut(|cfg| {
            let table = cfg
                .gambling_tables
                .get_mut(&user_id)
                .and_then(|tables| tables.get_mut(&table_id))
                .ok_or_eyre("No gambling table found with id {table_id}")?;

            // remove money from player's account
            let account = cfg.account.entry(user_id).or_default();
            ensure!(
                account.balance >= table.buyin,
                "You don't have enough money for a buy-in: {}",
                currency(cur, account.balance)
            );
            account.balance -= table.buyin;

            // add money to table
            let bet = table.players.entry(user_id).or_default();
            *bet += table.buyin;
            tracing::info!(
                "User {user_id} bought in for {} on table {}",
                currency(cur, table.buyin),
                table_id
            );
            tracing::info!("Pot prev: {}", table.pot);
            table.pot += table.buyin;
            tracing::info!("Pot now: {}", table.pot);

            Ok(table.clone())
        })
        .await?;

    let response = table.create_reply(cur, table_id).to_slash_initial_response(Default::default());
    component
        .create_response(ctx.serenity_context, CreateInteractionResponse::UpdateMessage(response))
        .await?;

    Ok(())
}

fn currency(symbol: &str, money: u64) -> String {
    format!("{money} {symbol}")
}

async fn get_currency(data: &impl With<ConfigT>) -> Result<String> {
    data.with_ok(|cfg| cfg.currency.clone()).await
}
