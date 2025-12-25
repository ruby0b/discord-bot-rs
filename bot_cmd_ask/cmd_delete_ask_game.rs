use crate::ConfigT;
use bot_core::{CmdContext, With};
use eyre::Result;
use itertools::Itertools;
use poise::CreateReply;
use poise::serenity_prelude::CreateAttachment;
use std::collections::BTreeMap;

/// Delete game-specific /ask ping and defaults
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD", default_member_permissions = "MANAGE_GUILD")]
pub async fn delete_ask_game<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Game name"]
    #[autocomplete = crate::autocomplete::existing_game_name]
    name: String,
) -> Result<()> {
    let deleted: BTreeMap<_, _> =
        ctx.data().with_mut_ok(|cfg| cfg.games.extract_if(.., |game_name, _| *game_name == name).collect()).await?;

    if deleted.is_empty() {
        ctx.say("❌ No game with that name was found").await?;
    } else {
        let attachment =
            CreateAttachment::bytes(deleted.iter().map(|game| format!("{game:?}")).join("\n"), "deleted.txt");
        let reply = CreateReply::new().content("🗑️ Ask defaults deleted").attachment(attachment);
        ctx.send(reply).await?;
    }
    Ok(())
}
