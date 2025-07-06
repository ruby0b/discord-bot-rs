use crate::{ACCOUNT_BUTTON_ID, ConfigT, Currency, DailyIncome, TABLE_SELECT_ID};
use bot_core::{CmdContext, CreateReplyExt, EvtContext, OptionExt as _, With, avatar_url};
use chrono::{DateTime, Datelike, Local, TimeZone};
use eyre::{OptionExt, Result};
use itertools::Itertools;
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, ChannelType, Colour, ComponentInteraction, ComponentInteractionDataKind,
    CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenu, CreateSelectMenuKind,
    CreateSelectMenuOption, Member,
};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Check your balance and claim your income
#[poise::command(slash_command, guild_only)]
pub async fn account<D: With<ConfigT>>(ctx: CmdContext<'_, D>, user: Option<Member>) -> Result<()> {
    let (reply, mut components) =
        account_reply(ctx.data(), ctx.author_member().await.some()?.as_ref(), user).await?;

    components.push(CreateActionRow::Buttons(vec![
        CreateButton::new(ACCOUNT_BUTTON_ID).style(ButtonStyle::Primary).label("/account"),
    ]));

    ctx.send(reply.components(components)).await?;

    Ok(())
}

pub async fn account_button(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
) -> Result<()> {
    let (reply, components) =
        account_reply(ctx.user_data, component.member.as_ref().some()?, None).await?;

    reply
        .components(components)
        .ephemeral(component.channel.as_ref().is_some_and(|c| c.kind != ChannelType::Voice))
        .respond_to_interaction(ctx.serenity_context, component)
        .await?;

    Ok(())
}

pub async fn table_select(
    ctx: EvtContext<'_, impl With<ConfigT>>,
    component: &ComponentInteraction,
) -> Result<()> {
    let ComponentInteractionDataKind::StringSelect { values } = &component.data.kind else {
        return Ok(());
    };

    let table_id = values.first().some()?.parse::<Uuid>()?;
    let cur = Currency::read(ctx.user_data).await?;
    let table = ctx
        .user_data
        .with(|cfg| cfg.gambling_tables.get(&table_id).cloned().ok_or_eyre("Table doesn't exist"))
        .await?;

    table.reply(&cur, table_id).respond_to_interaction(ctx.serenity_context, component).await?;

    Ok(())
}

async fn account_reply(
    data: &impl With<ConfigT>,
    author: &Member,
    member: Option<Member>,
) -> Result<(CreateReply, Vec<CreateActionRow>)> {
    let member = member.as_ref().unwrap_or(author);

    let cur = Currency::read(data).await?;
    let (account, rewarded_days, income, tables) = data
        .with_mut_ok(|cfg| {
            let account = cfg.account.entry(member.user.id).or_default();

            let now = Local::now();
            let last_claim = account.last_claim.map(|t| t.into());
            let rewarded_days = rewarded_days(&cfg.daily_income, last_claim, now);
            let income = (rewarded_days as u64) * cfg.daily_income.amount;

            // claim income for yourself
            if income != 0 && member.user.id == author.user.id {
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
        .thumbnail(avatar_url(member))
        .field("Balance", cur.fmt(account.balance).to_string(), true);

    if income != 0 {
        let income_str = format!(
            "{rewarded_days} day{}: +{}",
            if rewarded_days > 1 { "s" } else { "" },
            cur.fmt(income)
        );
        let title = if member.user.id == author.user.id { "Income" } else { "Uncollected Income" };
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
            CreateSelectMenu::new(TABLE_SELECT_ID, CreateSelectMenuKind::String { options })
                .min_values(1)
                .max_values(1)
                .placeholder("View a gambling table..."),
        ))
    }

    Ok((CreateReply::new().embed(embed), components))
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
