use crate::{ConfigT, GamblingTable, StateT, currency, get_currency};
use bot_core::{EvtContext, OptionExt as _, State, With};
use eyre::{OptionExt, Result, ensure};
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
        .with(|cfg| cfg.gambling_tables.get(&table_id).cloned().ok_or_eyre("Table doesn't exist"))
        .await?;

    ensure!(
        table.dealer == user_id,
        "You are not the dealer of this table. Only the dealer can pay out."
    );

    ensure!(!table.players.is_empty(), "No players to pay out.");

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
        let yaml = serde_yml::from_str::<BTreeMap<String, u64>>(
            modal_response.inputs.get(0).ok_or_eyre("No input value provided")?,
        )?;

        let factor = {
            let sum = yaml.values().sum::<u64>();
            table.pot as f64 / if sum != 0 { sum as f64 } else { table.players.len() as f64 }
        };

        let players_vec = table.players.keys().collect_vec();

        let mut map = yaml
            .into_iter()
            .filter_map(|(key, payout)| {
                let idx = key.split_once('@')?.0.parse::<usize>().ok()?;
                let payout = (payout as f64 * factor) as u64;
                Some((**players_vec.get(idx)?, payout))
            })
            .collect::<BTreeMap<_, _>>();

        // in case we have any money left over in the pot, suggest giving it to a winner
        let new_sum = map.values().sum::<u64>();
        let winner = map.iter_mut().max_by_key(|(_, amount)| **amount).some()?;
        *winner.1 += table.pot - new_sum;

        map
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

    let (msg, table) = {
        // we have to lock the table until the payout is confirmed and processed
        let mutex = ctx.user_data.state().table_locks.entry(table_id).or_default().clone();
        let _lock = mutex.lock().await;

        const CONFIRM_ID: &str = "~economy.confirm";
        const CANCEL_ID: &str = "~economy.cancel";

        let msg = {
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

            modal_response.interaction.get_response(ctx.serenity_context).await?
        };

        let confirm_interaction = msg
            .await_component_interaction(ctx.serenity_context)
            .timeout(Duration::from_secs(60))
            .await;

        CreateReply::new()
            .components(vec![])
            .embed(embed.clone().colour(Colour::DARKER_GREY))
            .to_prefix_edit(Default::default())
            .execute(ctx.serenity_context, (msg.channel_id, msg.id, Some(msg.author.id)))
            .await?;

        if confirm_interaction.map(|i| i.data.custom_id).is_none_or(|id| id != CONFIRM_ID) {
            return Ok(());
        }

        let table = ctx.user_data.with_mut(|cfg| pay_out(cfg, table_id, &payout_map)).await?;

        (msg, table)
    };

    // mark the payout message as successful with green
    CreateReply::new()
        .components(vec![])
        .embed(embed.colour(Colour::DARK_GREEN))
        .to_prefix_edit(Default::default())
        .execute(ctx.serenity_context, (msg.channel_id, msg.id, Some(msg.author.id)))
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
    let table = cfg.gambling_tables.get_mut(&table_id).ok_or_eyre("Table doesn't exist")?.clone();

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
