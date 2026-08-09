use crate::{ConfigT, Game, GameDefaults, StateT, worker_game_roles};
use bot_core::ext::option::OptionExt as _;
use bot_core::serde::LiteralRegex;
use bot_core::{CmdContext, State, With};
use eyre::{OptionExt as _, Result, WrapErr as _};
use fancy_regex::Regex;
use poise::serenity_prelude::prelude::Mentionable;
use poise::serenity_prelude::{EditRole, Permissions, RoleId};
use url::Url;

/// Edit game-specific /ask ping and defaults
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD", default_member_permissions = "MANAGE_GUILD")]
pub async fn configure_ask_game<D: With<ConfigT> + State<StateT>>(
    ctx: CmdContext<'_, D>,
    #[description = "Game name (a role with this name will be created)"]
    #[autocomplete = crate::autocomplete::existing_game_name]
    name: String,
    #[description = "Auto-assign the game role to everyone with this (category) role"] parent_role: RoleId,
    #[description = "Regex to match the game title"] title_pattern: Option<String>,
    #[description = "(Default) Minimum number of players"] min_players: Option<u32>,
    #[description = "(Default) Maximum number of players"] max_players: Option<u32>,
    #[description = "(Default) Link to the game"] url: Option<Url>,
    #[description = "(Default) Description of the game"] description: Option<String>,
    #[description = "(Default) Thumbnail of the game"] thumbnail_url: Option<String>,
) -> Result<()> {
    let guild_id = ctx.guild_id().some()?;

    ctx.defer().await?;

    let existing_game_role_id = if let Some(id) = {
        let guild = ctx.guild().some()?;
        crate::get_unique_role_by_name(&guild, name.trim())?
    } && ctx.data().with_ok(|cfg| cfg.games.contains_key(&id)).await?
    {
        Some(id)
    } else {
        None
    };

    let pattern = title_pattern.as_ref().unwrap_or(&name);
    let title_pattern = Regex::new(&format!("(?i){pattern}")).wrap_err("Invalid regex")?;

    let role_builder = {
        let guild = ctx.guild().some()?;
        let builder = EditRole::new().name(name.clone()).permissions(Permissions::empty());
        let parent_role = guild.roles.get(&parent_role).ok_or_eyre("Parent role not found")?;
        builder
            .colour(parent_role.colour)
            .mentionable(parent_role.mentionable)
            .audit_log_reason("Created game role from parent role")
    };

    let role_id = match existing_game_role_id {
        Some(id) => id,
        None => guild_id.create_role(ctx, role_builder).await?.id,
    };

    ctx.data()
        .with_mut_ok(|cfg| {
            cfg.games.insert(
                role_id,
                Game {
                    parent_role,
                    title_pattern: LiteralRegex(title_pattern),
                    defaults: GameDefaults { min_players, max_players, url, description, thumbnail_url },
                    opted_out_users: Default::default(),
                },
            );
        })
        .await?;

    ctx.say(format!("📝 Ask defaults updated, created role {}", role_id.mention())).await?;

    ctx.data().state().game_role_sender.get().some()?.send(worker_game_roles::Command::Update).await?;

    Ok(())
}
