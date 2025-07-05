use crate::{ConfigT, Currency, GamblingTable, StateT};
use bot_core::{CreateReplyExt, EvtContext, OptionExt as _, State, With, deferred_message, to_snd};
use eyre::{OptionExt, Result, ensure};
use itertools::Itertools;
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, Cache, Colour, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed,
    CreateInputText, CreateQuickModal, InputTextStyle, Mentionable as _, Message, ModalInteraction,
    QuickModalResponse, UserId,
};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use uuid::Uuid;

pub async fn pay_player_button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let cur = Currency::read(ctx.user_data).await?;
    let table_id = Uuid::try_parse(param)?;

    let prefix =
        "# Only keep players you want to pay out.\n# Enter the amount of money those player won.";
    let (table, modal) = payout_modal(&ctx, &cur, component, table_id, prefix).await?;
    let Some(modal) = modal else { return Ok(()) };

    let payouts = {
        let input_map = parse_payout(
            &ctx.serenity_context.cache,
            table.players.keys().copied(),
            modal.inputs.get(0).ok_or_eyre("No input")?,
        )?;

        table
            .players
            .keys()
            .copied()
            .map(to_snd(|user_id| input_map.get(user_id).copied()))
            .filter_map(|(k, v)| v.map(|v| (k, v)))
            .collect_vec()
    };

    payout_confirm(ctx, &cur, table_id, &table, &component.message, &modal.interaction, &payouts)
        .await?;

    Ok(())
}

pub async fn pay_table_button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let cur = Currency::read(ctx.user_data).await?;
    let table_id = Uuid::try_parse(param)?;

    let prefix = "# Enter the amount of money each player won";
    let (table, modal) = payout_modal(&ctx, &cur, component, table_id, prefix).await?;
    let Some(modal) = modal else { return Ok(()) };

    let payouts = {
        let input_map = parse_payout(
            &ctx.serenity_context.cache,
            table.players.keys().copied(),
            modal.inputs.get(0).ok_or_eyre("No input")?,
        )?;

        let factor = {
            let sum = input_map.iter().map(|x| x.1).sum::<u64>();
            table.pot as f64 / if sum != 0 { sum as f64 } else { table.players.len() as f64 }
        };

        let mut map = table
            .players
            .keys()
            .copied()
            .map(to_snd(|user_id| {
                let payout = input_map.get(user_id).copied().unwrap_or_default();
                (payout as f64 * factor) as u64
            }))
            .collect_vec();

        // in case we have any money left over in the pot, suggest giving it to a winner
        let new_sum = map.iter().map(|x| x.1).sum::<u64>();
        let winner = map.iter_mut().max_by_key(|(_, amount)| *amount).some()?;
        winner.1 += table.pot - new_sum;

        map
    };

    payout_confirm(ctx, &cur, table_id, &table, &component.message, &modal.interaction, &payouts)
        .await?;

    Ok(())
}

async fn payout_modal(
    ctx: &EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    cur: &Currency,
    component: &ComponentInteraction,
    table_id: Uuid,
    prefix: &str,
) -> Result<(GamblingTable, Option<QuickModalResponse>)> {
    let table = ctx
        .user_data
        .with(|cfg| cfg.gambling_tables.get(&table_id).cloned().ok_or_eyre("Table doesn't exist"))
        .await?;

    ensure!(table.dealer == component.user.id, "You are not the dealer of this table.");
    ensure!(!table.players.is_empty(), "No players to pay out.");

    let mut name_to_id = BTreeMap::new();
    for &user_id in table.players.keys() {
        let name = ctx.serenity_context.cache.user(user_id).some()?.name.clone();
        name_to_id.insert(name, user_id);
    }

    let template = {
        let mut players_string = String::new();
        for (user_id, bet) in &table.players {
            let name = &ctx.serenity_context.cache.user(user_id).some()?.name;
            players_string.push_str(&format!("\n{name}: {bet}"));
        }
        format!("{prefix}\n# Pot is {}\n{players_string}", cur.fmt(table.pot))
    };

    let modal = CreateQuickModal::new("Pay Out")
        .field(CreateInputText::new(InputTextStyle::Paragraph, "Pay Out", "").value(template))
        .timeout(Duration::from_secs(10 * 60))
        .execute(ctx.serenity_context, component.id, &component.token)
        .await?;

    Ok((table, modal))
}

async fn payout_confirm(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    cur: &Currency,
    table_id: Uuid,
    table: &GamblingTable,
    table_message: &Message,
    interaction: &ModalInteraction,
    payouts: &[(UserId, u64)],
) -> Result<()> {
    deferred_message(ctx.serenity_context, interaction).await?;

    // lock the table until the payout is done
    let _lock = ctx.user_data.state().table_locks.get(table_id);
    let _lock = _lock.lock().await;

    let summary = payouts
        .iter()
        .sorted_by(|(_, p1), (_, p2)| p2.cmp(p1))
        .map(|&(id, p)| format!("{} {}", id.mention(), cur.fmt(p)))
        .join("\n");
    let embed = CreateEmbed::new().title("Pay Out").description(summary).colour(Colour::GOLD);

    let confirm_id = "~economy.confirm";
    let cancel_id = "~economy.cancel";

    let message = CreateReply::new()
        .embed(embed.clone())
        .components(vec![CreateActionRow::Buttons(vec![
            CreateButton::new(confirm_id).label("Confirm").style(ButtonStyle::Success),
            CreateButton::new(cancel_id).label("Cancel").style(ButtonStyle::Danger),
        ])])
        .edit_interaction(ctx.serenity_context, interaction)
        .await?;

    let first_interaction = async {
        while let Some(interaction) = message
            .await_component_interaction(ctx.serenity_context)
            .timeout(Duration::from_secs(60))
            .await
        {
            if table.dealer == interaction.user.id {
                return Some(interaction);
            }
        }
        None
    }
    .await;

    // deactivate confirmation message in all cases
    CreateReply::new()
        .embed(embed.clone().colour(Colour::DARKER_GREY))
        .components(vec![])
        .edit_message(ctx.serenity_context, &message)
        .await?;

    if first_interaction.is_none_or(|i| i.data.custom_id != confirm_id) {
        return Ok(());
    }

    let table = ctx.user_data.with_mut(|cfg| apply_payout(cfg, table_id, payouts)).await?;

    CreateReply::new()
        .components(vec![])
        .embed(embed.colour(Colour::DARK_GREEN))
        .edit_message(ctx.serenity_context, &message)
        .await?;

    if table.pot == 0 { table.deactivated_reply(cur) } else { table.reply(cur, table_id) }
        .edit_message(ctx.serenity_context, table_message)
        .await?;

    Ok(())
}

fn parse_payout(
    cache: &Cache,
    players: impl IntoIterator<Item = UserId>,
    input: &str,
) -> Result<HashMap<UserId, u64>> {
    let mut name_to_id = HashMap::new();
    for user_id in players {
        let name = cache.user(user_id).some()?.name.clone();
        name_to_id.insert(name, user_id);
    }

    let mut map = HashMap::new();

    for line in input.lines() {
        let line = line.trim();

        // skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (user, amount) = line.split_once(':').ok_or_eyre("Invalid payout format")?;
        let user = name_to_id.get(user.trim()).copied().ok_or_eyre("Invalid user in payout")?;
        if let Ok(payout) = amount.trim().parse::<u64>() {
            *map.entry(user).or_default() += payout;
        }
    }

    ensure!(!map.is_empty(), "No payouts specified");
    Ok(map)
}

fn apply_payout(
    cfg: &mut ConfigT,
    table_id: Uuid,
    payouts: &[(UserId, u64)],
) -> Result<GamblingTable> {
    let table = cfg.gambling_tables.get_mut(&table_id).ok_or_eyre("Table doesn't exist")?;

    let payout_sum = payouts.iter().map(|x| x.1).sum::<u64>();

    ensure!(payout_sum <= table.pot, "Pay out sum exceeds the pot. ({payout_sum} > {})", table.pot);

    table.pot -= payout_sum;

    for &(player_id, payout) in payouts {
        let account = cfg.account.entry(player_id).or_default();
        account.balance += payout;
        table.players.remove(&player_id);
        tracing::info!(
            "User {} received {} from {}",
            player_id.mention(),
            cfg.currency.fmt(payout),
            table.name
        );
    }

    if table.pot == 0 {
        Ok(cfg.gambling_tables.remove(&table_id).some()?)
    } else {
        Ok(table.clone())
    }
}
