use crate::{ConfigT, Currency, Income};
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
    let (account, income_desc, tables) = ctx
        .data()
        .with_mut_ok(|cfg| {
            let account = cfg.account.entry(member.user.id).or_default();

            let now = Local::now();
            let last_claim = account.last_claim.map(|t| t.into());
            let income_desc = describe_income(&due_income(cfg.income, last_claim, now));

            // claim income for yourself
            if !income_desc.is_empty() && member.user.id == ctx.author().id {
                account.last_claim = Some(now.into());
                account.balance += income_desc.iter().map(|(_, v)| v).sum::<u64>();
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

            (account.clone(), income_desc, tables)
        })
        .await?;

    let mut embed = CreateEmbed::new()
        .title(member.display_name())
        .colour(Colour::BLITZ_BLUE)
        .thumbnail(avatar_url(&member))
        .field("Balance", cur.fmt(account.balance).to_string(), false);

    if !income_desc.is_empty() {
        let income_str = income_desc
            .into_iter()
            .map(|(interval, amount)| format!("{interval}: {}", cur.fmt(amount)))
            .join("\n");
        let title = if member.user.id == ctx.author().id { "Income" } else { "Uncollected Income" };
        embed = embed.field(title, income_str, false)
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

fn due_income<TZ: TimeZone>(
    mut income: Income,
    last_claim: Option<DateTime<TZ>>,
    now: DateTime<TZ>,
) -> Income {
    let Some(last_claim) = last_claim else { return income };

    if last_claim.num_days_from_ce() >= now.num_days_from_ce() {
        income.daily = 0;
    }
    if last_claim.year() >= now.year() && last_claim.iso_week() >= now.iso_week() {
        income.weekly = 0;
    }
    if last_claim.year() >= now.year() && last_claim.month() >= now.month() {
        income.monthly = 0;
    }

    income
}

fn describe_income(income: &Income) -> Vec<(String, u64)> {
    let mut desc = vec![];

    if income.daily > 0 {
        desc.push(("Daily".to_string(), income.daily));
    }
    if income.weekly > 0 {
        desc.push(("Weekly".to_string(), income.weekly));
    }
    if income.monthly > 0 {
        desc.push(("Monthly".to_string(), income.monthly));
    }

    desc
}
