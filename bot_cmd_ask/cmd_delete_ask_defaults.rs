use crate::ConfigT;
use bot_core::{CmdContext, With};
use eyre::Result;
use itertools::Itertools;
use poise::serenity_prelude::{self as serenity};
use std::collections::BTreeMap;

/// Delete /ask default values where a title is matched by the regex

#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn delete_ask_defaults<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Regex to match the game title"] title_pattern: String,
) -> Result<()> {
    #[allow(clippy::mutable_key_type)]
    let deleted = ctx
        .data()
        .with_mut_ok(|cfg| {
            cfg.defaults
                .extract_if(.., |regex, _| regex.0.is_match(&title_pattern).is_ok_and(|x| x))
                .collect::<BTreeMap<_, _>>()
        })
        .await?;

    if deleted.is_empty() {
        ctx.say("❌ No matching ask defaults were found").await?;
    } else {
        let attachment = serenity::CreateAttachment::bytes(
            deleted.iter().map(|(re, v)| format!("{:?}: {v:?}", re.0.as_str())).join("\n"),
            "deleted.txt",
        );
        let reply =
            poise::CreateReply::new().content("🗑️ Ask defaults deleted").attachment(attachment);
        ctx.send(reply).await?;
    }
    Ok(())
}
