use crate::{ConfigT, GamblingTable, StateT, currency, get_currency};
use bot_core::{CmdContext, EvtContext, OptionExt as _, State, With};
use eyre::{OptionExt, Result, bail, ensure};
use itertools::{Itertools, enumerate};
use poise::CreateReply;
use poise::serenity_prelude::{
    Builder, ButtonStyle, Colour, ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed,
    CreateInputText, CreateInteractionResponse, CreateQuickModal, InputTextStyle, Mentionable as _,
    UserId,
};
use std::collections::BTreeMap;
use std::time::Duration;
use uuid::Uuid;

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

    let payout_map = {
        let yaml_str = modal_response.inputs.get(0).ok_or_eyre("No input value provided")?;
        let yaml = serde_yml::from_str::<BTreeMap<String, u64>>(yaml_str)?;
        let players_vec = table.players.keys().collect_vec();
        yaml.into_iter()
            .filter_map(|(key, payout)| {
                let idx = key.split_once('@')?.0.parse::<usize>().ok()?;
                Some((**players_vec.get(idx)?, payout))
            })
            .collect::<BTreeMap<_, _>>()
    };

    let embed = {
        let summary = payout_map
            .iter()
            .map(|(&id, &amount)| format!("{}: {}", id.mention(), currency(cur, amount)))
            .join("\n");

        CreateEmbed::new().title("Pay Out").description(summary).colour(Colour::GOLD)
    };

    modal_response
        .interaction
        .create_response(ctx.serenity_context, CreateInteractionResponse::Defer(Default::default()))
        .await?;

    let (button_int, table) = {
        // we have to lock the table until the payout is confirmed and processed
        let mutex = ctx.user_data.state().table_locks.entry(table_id).or_default().clone();
        let _lock = mutex.lock().await;

        const CONFIRM_ID: &str = "~economy.confirm";
        const CANCEL_ID: &str = "~economy.cancel";

        let button_interaction = {
            let reply =
                CreateReply::new().embed(embed.clone()).components(vec![CreateActionRow::Buttons(
                    vec![
                        CreateButton::new(CONFIRM_ID).label("Confirm").style(ButtonStyle::Success),
                        CreateButton::new(CANCEL_ID).label("Cancel").style(ButtonStyle::Danger),
                    ],
                )]);

            modal_response
                .interaction
                .edit_response(
                    ctx.serenity_context,
                    reply.to_slash_initial_response_edit(Default::default()),
                )
                .await?;

            let msg = modal_response.interaction.get_response(ctx.serenity_context).await?;

            msg.await_component_interaction(ctx.serenity_context)
                .timeout(Duration::from_secs(60))
                .await
                .ok_or_eyre("Took too long to confirm")?
        };

        let id = button_interaction.data.custom_id.as_str();
        match id {
            CONFIRM_ID | CANCEL_ID => {
                button_interaction
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
            _ => bail!("Unexpected interaction id: {}", button_interaction.data.custom_id),
        }

        let table = ctx.user_data.with_mut(|cfg| pay_out(cfg, table_id, &payout_map)).await?;

        (button_interaction, table)
    };

    // mark the payout message as successful with green
    button_int
        .edit_response(
            ctx.serenity_context,
            CreateReply::new()
                .components(vec![])
                .embed(embed.colour(Colour::DARK_GREEN))
                .to_slash_initial_response_edit(Default::default()),
        )
        .await?;

    // update the table message as it is now deactived
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
