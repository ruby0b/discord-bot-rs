use bot_core::{CmdContext, With};
use chrono::{DateTime, Duration, Utc};
use eyre::Result;
use num_bigint::BigUint;
use poise::serenity_prelude::UserId;
use std::collections::BTreeMap;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
pub struct ConfigT {
    currency: String,
    #[default(Duration::days(1))]
    income_cooldown: Duration,
    #[default(BigUint::from(100u32))]
    income_amount: BigUint,
    account: BTreeMap<UserId, UserAccount>,
    gambling_tables: (),
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, sensible::Default)]
struct UserAccount {
    balance: BigUint,
    last_income: Option<DateTime<Utc>>,
}

/// Check your balance and claim your income
#[poise::command(slash_command, guild_only)]
pub async fn balance<D: With<ConfigT>>(ctx: CmdContext<'_, D>) -> Result<()> {
    let (account, income) = ctx
        .data()
        .with_mut_ok(|cfg| {
            let account = cfg.account.entry(ctx.author().id).or_default();
            let mut income = None;
            if account.last_income.is_none_or(|date| date < Utc::now() - cfg.income_cooldown) {
                income = Some(cfg.income_amount.clone());
                account.last_income = Some(Utc::now());
                account.balance += cfg.income_amount.clone();
            }
            (account.clone(), income)
        })
        .await?;

    ctx.say(format!(
        "Your balance is: {}{}",
        currency(ctx, account.balance).await?,
        income.map(|i| format!(" (received {i} as income)")).unwrap_or_default()
    ))
    .await?;

    Ok(())
}

async fn currency(ctx: CmdContext<'_, impl With<ConfigT>>, money: BigUint) -> Result<String> {
    let cur = ctx.data().with_ok(|cfg| cfg.currency.clone()).await?;
    Ok(format!("{money} {cur}"))
}
