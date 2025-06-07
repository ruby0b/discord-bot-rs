mod sandbox;

use crate::sandbox::run_in_sandbox;
use anyhow::Result;
use bot_core::{CmdContext, UserData};
use poise::CreateReply;
use poise::serenity_prelude::all::CreateAttachment;

/// Create a diagram using the d2 language
#[poise::command(slash_command, guild_only)]
pub async fn d2<D: UserData>(
    ctx: CmdContext<'_, D>,
    #[description = "d2 code"] code: String,
) -> Result<()> {
    ctx.defer().await?;
    let svg = run_in_sandbox("d2", &["-", "-"], code.as_bytes()).await?;
    let png = run_in_sandbox("magick", &["-", "png:-"], &svg).await?;
    ctx.send(CreateReply::new().attachment(CreateAttachment::bytes(png, "d2.png"))).await?;
    Ok(())
}

static PRELUDE: &str = r#"
#set page(width: auto, height: auto, margin: 5pt)
"#;

/// Render text using typst
#[poise::command(slash_command, guild_only)]
pub async fn typst<D: UserData>(
    ctx: CmdContext<'_, D>,
    #[description = "typst code"] code: String,
) -> Result<()> {
    ctx.defer().await?;
    let args = ["compile", "--format=png", "-", "-"];
    let stdin = format!("{PRELUDE}\n{code}");
    let png = run_in_sandbox("typst", &args, stdin.as_bytes()).await?;
    ctx.send(CreateReply::new().attachment(CreateAttachment::bytes(png, "typst.png"))).await?;
    Ok(())
}

/// Render math using typst
#[poise::command(slash_command, guild_only)]
pub async fn math<D: UserData>(
    ctx: CmdContext<'_, D>,
    #[description = "typst math code"] code: String,
) -> Result<()> {
    ctx.defer().await?;
    let args = ["compile", "--format=png", "-", "-"];
    let stdin = format!("{PRELUDE}\n$ {code} $");
    let png = run_in_sandbox("typst", &args, stdin.as_bytes()).await?;
    ctx.send(CreateReply::new().attachment(CreateAttachment::bytes(png, "typst.png"))).await?;
    Ok(())
}
