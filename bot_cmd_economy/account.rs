use crate::{ConfigT, Currency, DailyIncome};
use bot_core::{CmdContext, OptionExt as _, With, avatar_url};
use chrono::{DateTime, Datelike, Local, TimeZone};
use eyre::{OptionExt, Result};
use itertools::Itertools;
use poise::CreateReply;
use poise::serenity_prelude::{
    Colour, ComponentInteraction, ComponentInteractionDataKind, CreateActionRow, CreateEmbed,
    CreateInteractionResponse, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption,
    Member,
};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Check your balance and claim your income
#[poise::command(slash_command, guild_only)]
pub async fn account<D: With<ConfigT>>(ctx: CmdContext<'_, D>, user: Option<Member>) -> Result<()> {
    let member = match user {
        Some(m) => m,
        None => ctx.author_member().await.some()?.into_owned(),
    };

    let cur = Currency::read(ctx.data()).await?;
    let (account, rewarded_days, income, tables) = ctx
        .data()
        .with_mut_ok(|cfg| {
            let account = cfg.account.entry(member.user.id).or_default();

            let now = Local::now();
            let last_claim = account.last_claim.map(|t| t.into());
            let rewarded_days = rewarded_days(&cfg.daily_income, last_claim, now);
            let income = (rewarded_days as u64) * cfg.daily_income.amount;

            // claim income for yourself
            if income != 0 && member.user.id == ctx.author().id {
                account.last_claim = Some(now.into());
                account.balance += income;
            }

            // tables the user is involved in
            let tables = cfg
                .gambling_tables
                .iter()
                .filter(|(_, t)| {
                    t.players.contains_key(&member.user.id) || t.dealer == member.user.id
                })
                .map(|(id, t)| (*id, t.clone()))
                .collect::<BTreeMap<_, _>>();

            (account.clone(), rewarded_days, income, tables)
        })
        .await?;

    let mut embed = CreateEmbed::new()
        .title(member.display_name())
        .colour(Colour::BLITZ_BLUE)
        .thumbnail(avatar_url(&member))
        .field("Balance", cur.fmt(account.balance).to_string(), true);

    if income != 0 {
        let income_str = format!(
            "{rewarded_days} day{}: +{}",
            if rewarded_days > 1 { "s" } else { "" },
            cur.fmt(income)
        );
        let title = if member.user.id == ctx.author().id { "Income" } else { "Uncollected Income" };
        embed = embed.field(title, income_str, true)
    }

    let mut components = vec![];

    // add a selection menu to view tables you're involved in
    if !tables.is_empty() {
        let options = tables
            .iter()
            .map(|(&id, t)| {
                CreateSelectMenuOption::new(t.name.clone(), id).description(format!(
                    "Buy-in: {} — Pot: {}",
                    cur.fmt(t.buyin),
                    cur.fmt(t.pot)
                ))
            })
            .collect_vec();

        components.push(CreateActionRow::SelectMenu(
            CreateSelectMenu::new("~economy.table", CreateSelectMenuKind::String { options })
                .min_values(1)
                .max_values(1)
                .placeholder("View a gambling table..."),
        ))
    }

    let handle = ctx.send(CreateReply::new().embed(embed).components(components)).await?;
    let message = handle.message().await?;

    while let Some(int) = message.await_component_interaction(ctx).await {
        if let ComponentInteractionDataKind::StringSelect { values } = &int.data.kind {
            handle_table_select(&ctx, &int, values).await?
        }
    }

    Ok(())
}

async fn handle_table_select(
    ctx: &CmdContext<'_, impl With<ConfigT>>,
    interaction: &ComponentInteraction,
    values: &[String],
) -> Result<()> {
    let table_id = values.first().some()?.parse::<Uuid>()?;
    let cur = Currency::read(ctx.data()).await?;
    let table = ctx
        .data()
        .with(|cfg| cfg.gambling_tables.get(&table_id).cloned().ok_or_eyre("Table doesn't exist"))
        .await?;

    let response = table.reply(&cur, table_id).to_slash_initial_response(Default::default());
    interaction.create_response(ctx, CreateInteractionResponse::Message(response)).await?;

    Ok(())
}

fn rewarded_days<TZ: TimeZone>(
    income: &DailyIncome,
    last_claim: Option<DateTime<TZ>>,
    now: DateTime<TZ>,
) -> u32 {
    let Some(last_claim) = last_claim else { return income.grace_period_days };
    let days_passed = now.num_days_from_ce() - last_claim.num_days_from_ce();
    days_passed.clamp(0, income.grace_period_days as i32) as u32
}
