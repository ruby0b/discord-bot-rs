use crate::{AskDefaults, ConfigT};
use bot_core::serde::LiteralRegex;
use bot_core::{CmdContext, With};
use eyre::{Result, WrapErr as _};
use fancy_regex::Regex;
use url::Url;

/// Add /ask default values for matching game titles
#[poise::command(
    slash_command,
    required_permissions = "MANAGE_GUILD",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn new_ask_defaults<D: With<ConfigT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Regex to match the game title"] title_pattern: String,
    #[description = "(Default) Minimum number of players"] min_players: Option<u32>,
    #[description = "(Default) Maximum number of players"] max_players: Option<u32>,
    #[description = "(Default) Link to the game"] url: Option<Url>,
    #[description = "(Default) Description of the game"] description: Option<String>,
    #[description = "(Default) Thumbnail of the game"] thumbnail_url: Option<String>,
) -> Result<()> {
    let title_pattern = Regex::new(&format!("(?i){title_pattern}")).wrap_err("Invalid regex")?;

    ctx.data()
        .with_mut_ok(|cfg| {
            cfg.defaults.insert(
                LiteralRegex(title_pattern),
                AskDefaults { min_players, max_players, url, description, thumbnail_url },
            );
        })
        .await?;

    ctx.say("📝 Ask defaults updated").await?;

    Ok(())
}
