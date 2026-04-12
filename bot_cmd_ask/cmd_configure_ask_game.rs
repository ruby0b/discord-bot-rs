use crate::{ConfigT, Game, GameDefaults, StateT, worker_game_roles};
use bot_core::serde::LiteralRegex;
use bot_core::{CmdContext, OptionExt, State, With};
use eyre::{Result, WrapErr as _};
use fancy_regex::Regex;
use poise::serenity_prelude::{Role, RoleId};
use url::Url;

/// Edit game-specific /ask ping and defaults
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD", default_member_permissions = "MANAGE_GUILD")]
pub async fn configure_ask_game<D: With<ConfigT> + State<StateT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Game name (a role with this name will be created)"]
    #[autocomplete = crate::autocomplete::existing_game_name]
    name: String,
    #[description = "Auto-assign the game role to everyone with this (category) role"] parent_role: Role,
    #[description = "Regex to match the game title"] title_pattern: Option<String>,
    #[description = "(Default) Minimum number of players"] min_players: Option<u32>,
    #[description = "(Default) Maximum number of players"] max_players: Option<u32>,
    #[description = "(Default) Link to the game"] url: Option<Url>,
    #[description = "(Default) Description of the game"] description: Option<String>,
    #[description = "(Default) Thumbnail of the game"] thumbnail_url: Option<String>,
) -> Result<()> {
    let pattern = title_pattern.as_ref().unwrap_or(&name);
    let title_pattern = Regex::new(&format!("(?i){pattern}")).wrap_err("Invalid regex")?;

    ctx.data()
        .with_mut_ok(|cfg| {
            cfg.games.insert(
                name,
                Game {
                    parent_role: parent_role.name,
                    title_pattern: LiteralRegex(title_pattern),
                    defaults: GameDefaults { min_players, max_players, url, description, thumbnail_url },
                    opted_out_users: Default::default(),
                },
            );
        })
        .await?;

    ctx.say("📝 Ask defaults updated").await?;

    ctx.data().state().game_role_sender.get().some()?.send(worker_game_roles::Command::Update).await?;

    Ok(())
}
