use bot_core::{CmdContext, EvtContext, OptionExt as _, State, With, avatar_url};
use chrono::{DateTime, TimeDelta, Utc};
use dashmap::DashMap;
use eyre::{OptionExt, Result, bail, ensure};
use itertools::{Itertools, enumerate};
use poise::CreateReply;
use poise::serenity_prelude::{
    Builder, ButtonStyle, Colour, ComponentInteraction, ComponentInteractionDataKind,
    CreateActionRow, CreateButton, CreateEmbed, CreateInputText, CreateInteractionResponse,
    CreateQuickModal, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption,
    InputTextStyle, Member, Mentionable as _, UserId,
};
use serde_with::{DisplayFromStr, serde_as};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
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
        .with(|cfg| {
            cfg.gambling_tables
                .get(&table_id)
                .cloned()
                .ok_or_eyre("No gambling table found with id {table_id}")
        })
        .await?;

    let response = table.reply(cur, table_id).to_slash_initial_response(Default::default());
    interaction.create_response(ctx, CreateInteractionResponse::Message(response)).await?;

    Ok(())
}

/// Create a new gambling table
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
            "{}'s {} Table",
            ctx.author_member().await.some()?.display_name(),
            name.map(|n| format!("{n} ")).unwrap_or_default(),
        ),
        buyin,
        dealer: ctx.author().id,
        players: Default::default(),
        pot: 0,
    };

    let reply = table.reply(cur, id);

    ctx.data().with_mut_ok(|cfg| cfg.gambling_tables.insert(id, table)).await?;

    ctx.send(reply).await?;

    Ok(())
}

pub async fn buyin_button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let user_id = component.user.id;
    let cur = &get_currency(ctx.user_data).await?;
    let table_id = Uuid::try_parse(param)?;

    component.defer(ctx.serenity_context).await?;

    let table = {
        let lock = ctx.user_data.state().table_locks.entry(table_id).or_default().clone();
        let _lock = lock.lock().await;
        ctx.user_data.with_mut(|cfg| buy_in(cfg, table_id, user_id, cur)).await?
    };

    component
        .edit_response(
            ctx.serenity_context,
            table.reply(cur, table_id).to_slash_initial_response_edit(Default::default()),
        )
        .await?;

    Ok(())
}

fn buy_in(
    cfg: &mut ConfigT,
    table_id: Uuid,
    user_id: UserId,
    cur: &str,
) -> std::result::Result<GamblingTable, eyre::Error> {
    let table = cfg.gambling_tables.get_mut(&table_id).ok_or_eyre("No such table found")?;

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
    table.pot += table.buyin;

    Ok(table.clone())
}

pub async fn payout_button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let user_id = component.user.id;
    let guild_id = component.guild_id.some()?;
    let cur = &get_currency(ctx.user_data).await?;
    let table_id = Uuid::try_parse(param)?;

    let table = ctx
        .user_data
        .with(|cfg| cfg.gambling_tables.get(&table_id).cloned().ok_or_eyre("No such table found"))
        .await?;

    ensure!(
        table.dealer == user_id,
        "You are not the dealer of this table. Only the dealer can pay out."
    );

    let players_vec = table.players.keys().collect_vec();
    let template = {
        let mut players = BTreeMap::new();
        for (idx, (id, bet)) in enumerate(&table.players) {
            let guild = ctx.serenity_context.cache.guild(guild_id).some()?;
            let name = guild.members.get(id).some()?.display_name().to_string();
            players.insert(format!("{idx}@{name}"), bet);
        }
        let yaml_template = serde_yml::to_string(&players)?;
        format!(
            "# Enter the amount of money each player won\n# Pot is {}\n\n{yaml_template}",
            currency(cur, table.pot)
        )
    };

    let Some(modal_response) = CreateQuickModal::new("Pay Out")
        .field(CreateInputText::new(InputTextStyle::Paragraph, "Pay Out", "").value(template))
        .timeout(Duration::from_secs(10 * 60))
        .execute(ctx.serenity_context, component.id, &component.token)
        .await?
    else {
        return Ok(());
    };

    let yaml_str = modal_response.inputs.get(0).ok_or_eyre("No input value provided")?;
    let yaml = serde_yml::from_str::<BTreeMap<String, u64>>(yaml_str)?;
    let payout_map = yaml
        .into_iter()
        .filter_map(|(key, payout)| {
            let idx = key.split_once('@')?.0.parse::<usize>().ok()?;
            Some((**players_vec.get(idx)?, payout))
        })
        .collect::<BTreeMap<_, _>>();

    let summary = payout_map
        .iter()
        .map(|(&id, &amount)| format!("{}: {}", id.mention(), currency(cur, amount)))
        .join("\n");
    let embed = CreateEmbed::new().title("Pay Out").description(summary).colour(Colour::GOLD);

    const CONFIRM_ID: &str = "~economy.confirm";
    const CANCEL_ID: &str = "~economy.cancel";
    let msg =
        CreateReply::new().embed(embed.clone()).components(vec![CreateActionRow::Buttons(vec![
            CreateButton::new(CONFIRM_ID).label("Confirm").style(ButtonStyle::Success),
            CreateButton::new(CANCEL_ID).label("Cancel").style(ButtonStyle::Danger),
        ])]);

    modal_response
        .interaction
        .create_response(ctx.serenity_context, CreateInteractionResponse::Defer(Default::default()))
        .await?;

    let (button_int, table) = {
        // we have to lock the table until the payout is confirmed and processed
        let lock = ctx.user_data.state().table_locks.entry(table_id).or_default().clone();
        let _lock = lock.lock().await;

        modal_response
            .interaction
            .edit_response(
                ctx.serenity_context,
                msg.to_slash_initial_response_edit(Default::default()),
            )
            .await?;
        let response_msg = modal_response.interaction.get_response(ctx.serenity_context).await?;

        let button_int = response_msg
            .await_component_interaction(ctx.serenity_context)
            .timeout(Duration::from_secs(60))
            .await
            .ok_or_eyre("Took too long to confirm")?;

        let id = button_int.data.custom_id.as_str();
        match id {
            CONFIRM_ID | CANCEL_ID => {
                button_int
                    .create_response(
                        ctx.serenity_context,
                        CreateInteractionResponse::UpdateMessage(
                            CreateReply::new()
                                .components(vec![])
                                .embed(embed.clone().colour(Colour::DARKER_GREY))
                                .to_slash_initial_response(Default::default()),
                        ),
                    )
                    .await?;
                if id == CANCEL_ID {
                    return Ok(());
                }
            }
            _ => bail!("Unexpected interaction id: {}", button_int.data.custom_id),
        }

        let table = ctx.user_data.with_mut(|cfg| pay_out(cfg, table_id, &payout_map)).await?;

        (button_int, table)
    };

    button_int
        .edit_response(
            ctx.serenity_context,
            CreateReply::new()
                .components(vec![])
                .embed(embed.colour(Colour::DARK_GREEN))
                .to_slash_initial_response_edit(Default::default()),
        )
        .await?;

    table
        .deactivated_reply(cur)
        .to_prefix_edit(Default::default())
        .execute(
            ctx.serenity_context,
            (component.message.channel_id, component.message.id, Some(component.message.author.id)),
        )
        .await?;

    Ok(())
}

fn pay_out(
    cfg: &mut ConfigT,
    table_id: Uuid,
    payout_map: &BTreeMap<UserId, u64>,
) -> Result<GamblingTable> {
    let table = cfg.gambling_tables.get_mut(&table_id).ok_or_eyre("No such table found")?.clone();

    let payout_sum = payout_map.values().sum::<u64>();
    ensure!(
        payout_sum == table.pot,
        format!("Pay out sum needs to match the pot. ({payout_sum} != {})", table.pot)
    );

    for (&player_id, &payout) in payout_map {
        let account = cfg.account.entry(player_id).or_default();
        account.balance += payout;
        tracing::info!(
            "User {player_id} received {} from table {table_id}",
            currency(&cfg.currency, payout)
        );
    }

    cfg.gambling_tables.remove(&table_id);

    Ok(table)
}

fn currency(symbol: &str, money: u64) -> String {
    format!("{money} {symbol}")
}

async fn get_currency(data: &impl With<ConfigT>) -> Result<String> {
    data.with_ok(|cfg| cfg.currency.clone()).await
}
