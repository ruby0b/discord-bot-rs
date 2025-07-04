use crate::{ConfigT, Currency, GamblingTable};
use bot_core::{CmdContext, OptionExt as _, With};
use eyre::Result;
use uuid::Uuid;

/// Create a new gambling table
#[poise::command(slash_command, guild_only)]
pub async fn gamble<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Buy-in for the table"] buyin: u64,
    #[description = "Name of the gambling table"] name: Option<String>,
    #[description = "Description"] description: Option<String>,
) -> Result<()> {
    let cur = Currency::read(ctx.data()).await?;

    let id = Uuid::new_v4();
    let table = GamblingTable {
        name: format!(
            "{}'s {}Table",
            ctx.author_member().await.some()?.display_name(),
            name.map(|n| format!("{n} ")).unwrap_or_default(),
        ),
        description,
        buyin,
        dealer: ctx.author().id,
        players: Default::default(),
        pot: 0,
    };

    let reply = table.reply(&cur, id);

    ctx.data().with_mut_ok(|cfg| cfg.gambling_tables.insert(id, table)).await?;

    ctx.send(reply).await?;

    Ok(())
}
