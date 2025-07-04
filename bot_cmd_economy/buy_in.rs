use crate::{ConfigT, Currency, GamblingTable, StateT};
use bot_core::{EvtContext, State, With};
use eyre::{OptionExt, Result, ensure};
use poise::serenity_prelude::{ComponentInteraction, UserId};
use uuid::Uuid;

pub async fn buyin_button_pressed(
    ctx: EvtContext<'_, impl With<ConfigT> + State<StateT>>,
    component: &ComponentInteraction,
    param: &str,
) -> Result<()> {
    let user_id = component.user.id;
    let cur = Currency::read(ctx.user_data).await?;
    let table_id = Uuid::try_parse(param)?;

    component.defer(ctx.serenity_context).await?;

    let table = {
        let _lock = ctx.user_data.state().table_locks.get(table_id);
        let _lock = _lock.lock().await;
        ctx.user_data.with_mut(|cfg| buy_in(cfg, &cur, table_id, user_id)).await?
    };

    component
        .edit_response(
            ctx.serenity_context,
            table.reply(&cur, table_id).to_slash_initial_response_edit(Default::default()),
        )
        .await?;

    Ok(())
}

fn buy_in(
    cfg: &mut ConfigT,
    cur: &Currency,
    table_id: Uuid,
    user_id: UserId,
) -> std::result::Result<GamblingTable, eyre::Error> {
    let table = cfg.gambling_tables.get_mut(&table_id).ok_or_eyre("Table doesn't exist")?;

    // remove money from player's account
    let account = cfg.account.entry(user_id).or_default();
    ensure!(
        account.balance >= table.buyin,
        "You don't have enough money for a buy-in: {}",
        cur.fmt(account.balance)
    );
    account.balance -= table.buyin;

    // add money to table
    let bet = table.players.entry(user_id).or_default();
    *bet += table.buyin;
    tracing::info!("User {user_id} bought in for {} on table {}", cur.fmt(table.buyin), table_id);
    table.pot += table.buyin;

    Ok(table.clone())
}
