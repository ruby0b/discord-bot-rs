use crate::{ConfigT, Game, GameDefaults, StateT, worker_game_roles};
use bot_core::ext::option::OptionExt as _;
use bot_core::serde::LiteralRegex;
use bot_core::{CmdContext, State, With};
use eyre::{OptionExt as _, Result, WrapErr as _, ensure};
use fancy_regex::Regex;
use itertools::Itertools;
use poise::CreateReply;
use poise::serenity_prelude::{EditRole, Guild, Mentionable, Permissions, RoleId};
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
    #[string]
    #[description = "(Default) Link to the game"]
    url: Option<Url>,
    #[description = "(Default) Description of the game"] description: Option<String>,
    #[description = "(Default) Thumbnail of the game"] thumbnail_url: Option<String>,
) -> Result<()> {
    let guild_id = ctx.guild_id().some()?;

    ctx.defer().await?;

    let game_roles = ctx
        .data()
        .with(|cfg| {
            let guild = ctx.guild().some()?;
            Ok(crate::game_roles(cfg, &guild).into_iter().collect_vec())
        })
        .await?;

    let title_pattern = title_pattern.as_ref().unwrap_or(&name);
    let title_pattern = Regex::new(&format!("(?i){title_pattern}")).wrap_err("Invalid regex")?;

    ensure!(title_pattern.is_match(&name)?, "Pattern has to match the game name");

    let existing_games_matched = game_roles
        .iter()
        .filter(|r| r.name != name)
        .filter(|r| title_pattern.is_match(&r.name).is_ok_and(|x| x))
        .collect_vec();
    ensure!(
        existing_games_matched.is_empty(),
        "Pattern can't match existing game names: {}",
        existing_games_matched.iter().map(|r| r.mention()).join(" ")
    );

    let role_id = if let Some(id) = {
        let guild = ctx.guild().some()?;
        crate::get_unique_role_by_name(&guild, name.trim())?
    } {
        id
    } else {
        let role_builder = {
            let guild = ctx.guild().some()?;
            build_child_role(&guild, name, parent_role)?
        };
        guild_id.create_role(ctx, role_builder).await?.id
    };

    let description = match (description, &url) {
        (Some(description), _) => Some(description),
        (None, Some(url)) => Some(crate::fetch_description(url).await?),
        (None, None) => None,
    };

    let game = Game {
        parent_role,
        title_pattern: LiteralRegex(title_pattern),
        defaults: GameDefaults { min_players, max_players, url, description, thumbnail_url },
        opted_out_users: Default::default(),
    };

    ctx.data().with_mut_ok(|cfg| cfg.games.insert(role_id, game)).await?;

    ctx.send(CreateReply::new().content(format!("📝 Game configured with role {}", role_id.mention()))).await?;

    ctx.data().state().game_role_sender.get().some()?.send(worker_game_roles::Command::Update).await?;

    Ok(())
}

fn build_child_role(guild: &Guild, name: String, parent: RoleId) -> Result<EditRole<'static>> {
    let builder = EditRole::new().name(name.clone()).permissions(Permissions::empty());
    let parent = guild.roles.get(&parent).ok_or_eyre("Parent role not found")?;
    Ok(builder
        .colour(parent.colour)
        .mentionable(parent.mentionable)
        .audit_log_reason("Created game role from parent role"))
}
