use crate::{ConfigT, currency, get_currency};
use bot_core::{CmdContext, OptionExt as _, With, avatar_url};
use chrono::Utc;
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

    let cur = &get_currency(ctx.data()).await?;
    let (account, income, tables) = ctx
        .data()
        .with_mut_ok(|cfg| {
            let account = cfg.account.entry(member.user.id).or_default();
            let mut income = None;
            if account.last_income.is_none_or(|date| date < Utc::now() - cfg.income_cooldown) {
                income = Some(cfg.income_amount);
                // only claim income for yourself
                if member.user.id == ctx.author().id {
                    account.last_income = Some(Utc::now());
                    account.balance += cfg.income_amount;
                }
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

            (account.clone(), income, tables)
        })
        .await?;

    let embed = {
        let income_str = income
            .map(|i| {
                if member.user.id == ctx.author().id {
                    format!(" (collected {} income)", currency(cur, i))
                } else {
                    format!(" ({} uncollected income)", currency(cur, i))
                }
            })
            .unwrap_or_default();

        CreateEmbed::new()
            .title(member.display_name())
            .colour(Colour::BLITZ_BLUE)
            .field("Balance", format!("{}{income_str}", currency(cur, account.balance)), true)
            .thumbnail(avatar_url(member))
    };

    let mut components = vec![];

    // add a selection menu to view tables you're involved in
    if !tables.is_empty() {
        let options = tables
            .iter()
            .map(|(&id, t)| {
                CreateSelectMenuOption::new(t.name.clone(), id).description(format!(
                    "Buy-in: {} — Pot: {}",
                    currency(cur, t.buyin),
                    currency(cur, t.pot)
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
    let cur = &get_currency(ctx.data()).await?;
    let table = ctx
        .data()
        .with(|cfg| cfg.gambling_tables.get(&table_id).cloned().ok_or_eyre("Table doesn't exist"))
        .await?;

    let response = table.reply(cur, table_id).to_slash_initial_response(Default::default());
    interaction.create_response(ctx, CreateInteractionResponse::Message(response)).await?;

    Ok(())
}
