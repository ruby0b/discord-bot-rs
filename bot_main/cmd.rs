use bot_core::{CmdContext, UserData};
use eyre::{OptionExt as _, Result, bail};
use poise::serenity_prelude::ReactionType;

/// Manage slash commands
#[poise::command(prefix_command, owners_only)]
pub async fn register<D: UserData>(ctx: CmdContext<'_, D>) -> Result<()> {
    poise::builtins::register_application_commands_buttons(ctx).await?;
    Ok(())
}

/// Unregister and re-register all guild commands
#[poise::command(prefix_command, owners_only, aliases("rr"))]
pub async fn reregister<D: UserData>(ctx: CmdContext<'_, D>) -> Result<()> {
    let guild_id = ctx.guild_id().ok_or_eyre("Must be called in guild")?;
    let create_commands =
        poise::builtins::create_application_commands(&ctx.framework().options().commands);

    guild_id.set_commands(ctx, vec![]).await?;
    guild_id.set_commands(ctx, create_commands).await?;

    match ctx {
        poise::Context::Application(_) => bail!("Can only be called in guild"),
        poise::Context::Prefix(prefix_context) => {
            prefix_context.msg.react(ctx, ReactionType::from('✅')).await?;
        }
    }

    Ok(())
}
