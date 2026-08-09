use crate::ConfigT;
use bot_core::ext::option::OptionExt as _;
use bot_core::{CmdContext, With};
use eyre::{OptionExt, Result};

/// Delete game-specific /ask ping and defaults
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD", default_member_permissions = "MANAGE_GUILD")]
pub async fn delete_ask_game<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Game name"]
    #[autocomplete = crate::autocomplete::existing_game_name]
    name: String,
) -> Result<()> {
    let role_id = {
        let guild = ctx.guild().some()?;
        crate::get_unique_role_by_name(&guild, name.trim())?.ok_or_eyre("No role with that name exists")?
    };

    ctx.defer().await?;

    let deleted_game = ctx.data().with_mut_ok(|cfg| cfg.games.remove(&role_id)).await?;
    let deleted_game = deleted_game.ok_or_eyre("No game with that name was found")?;

    ctx.guild_id().some()?.delete_role(ctx, role_id).await?;

    ctx.say(format!("🗑️ Deleted game: {deleted_game:?}")).await?;

    Ok(())
}
