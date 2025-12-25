use crate::{ConfigT, Game};
use bot_core::{OptionExt, State, With, safe_name};
use eyre::Result;
use itertools::Itertools;
use poise::serenity_prelude::{Builder as _, Context, EditRole, GuildId, Member, RoleId, UserId};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc;

#[derive(Debug)]
pub(crate) enum Command {
    Update,
}

pub(crate) async fn work(ctx: Context, data: impl With<ConfigT> + State<GuildId>, mut rx: mpsc::Receiver<Command>) {
    loop {
        if let Err(error) = select! {
            Some(cmd) = rx.recv() => {
                match cmd {
                    Command::Update => {
                        tracing::debug!("Explicit update");
                        update(&ctx, &data).await
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                tracing::trace!("Periodic update");
                update(&ctx, &data).await
            }
        } {
            tracing::error!("Error in worker: {error:?}");
        }
    }
}

const ROLE_CREATIONS_PER_MINUTE: usize = 3;
const ROLE_ADD_REMOVE_PER_MINUTE: usize = 20;

async fn update(ctx: &Context, data: &(impl With<ConfigT> + State<GuildId>)) -> Result<()> {
    let guild_id: GuildId = *data.state();
    let games = data.with_ok(|c| c.games.clone()).await?;

    let mut existing_game_roles: BTreeMap<String, RoleId> = {
        let guild = ctx.cache.guild(guild_id).some()?;
        games.keys().filter_map(|name| guild.role_by_name(name).map(|game_role| (name.clone(), game_role.id))).collect()
    };

    // only do a few role creations at a time to not send too many requests at once
    let create_role_requests = build_create_role_requests(ctx, guild_id, &games, &existing_game_roles)?;
    for (name, create_role) in create_role_requests.into_iter().take(ROLE_CREATIONS_PER_MINUTE) {
        tracing::info!("Creating game role {create_role:?}");
        let game_role = create_role.execute(ctx, (guild_id, None)).await?;
        existing_game_roles.insert(name, game_role.id);
    }

    // prioritize removing roles from users (opt-out) and use the remaining request budget for adding roles to users
    let role_remove_requests =
        build_role_requests(ctx, guild_id, &games, &existing_game_roles, |member, game, role_id| {
            game.opted_out_users.contains(&member.user.id) && member.roles.contains(&role_id)
        })?;
    let requests_per_minute_left = ROLE_ADD_REMOVE_PER_MINUTE.saturating_sub(role_remove_requests.len());
    for (user_id, role_id, name) in role_remove_requests.into_iter().take(ROLE_ADD_REMOVE_PER_MINUTE) {
        tracing::info!("Removing role {name} from member {} due to opt-out", safe_name(ctx, user_id));
        ctx.http.add_member_role(guild_id, user_id, role_id, Some("Game role opt-out")).await?;
    }

    let role_add_requests =
        build_role_requests(ctx, guild_id, &games, &existing_game_roles, |member, game, role_id| {
            !game.opted_out_users.contains(&member.user.id)
                && member.roles.contains(&game.parent_role)
                && !member.roles.contains(&role_id)
        })?;
    for (user_id, role_id, name) in role_add_requests.into_iter().take(requests_per_minute_left) {
        tracing::info!("Adding role {name} to member {}", safe_name(ctx, user_id));
        ctx.http.add_member_role(guild_id, user_id, role_id, Some("Game role")).await?;
    }

    Ok(())
}

fn build_create_role_requests(
    ctx: &Context,
    guild_id: GuildId,
    games: &BTreeMap<String, Game>,
    existing_game_roles: &BTreeMap<String, RoleId>,
) -> Result<Vec<(String, EditRole<'static>)>> {
    let guild = ctx.cache.guild(guild_id).some()?;
    Ok(games
        .iter()
        .filter(|(name, _)| !existing_game_roles.contains_key(name.as_str()))
        .map(|(name, game)| {
            let builder = EditRole::new().name(name.clone());
            let builder = match guild.roles.get(&game.parent_role) {
                Some(parent_role) => builder
                    .colour(parent_role.colour)
                    .mentionable(parent_role.mentionable)
                    .audit_log_reason("Created game role from parent role"),
                None => builder.audit_log_reason("Created game role (could not find parent role so no defaults!)"),
            };
            (name.clone(), builder)
        })
        .collect())
}

fn build_role_requests(
    ctx: &Context,
    guild_id: GuildId,
    games: &BTreeMap<String, Game>,
    game_roles: &BTreeMap<String, RoleId>,
    mut filter: impl FnMut(&Member, &Game, RoleId) -> bool,
) -> Result<Vec<(UserId, RoleId, String)>> {
    let guild = ctx.cache.guild(guild_id).some()?;
    game_roles
        .iter()
        .map(|(name, &role_id)| {
            let game = games.get(name).some()?;
            let requests = guild
                .members
                .values()
                .filter(|member| filter(member, game, role_id))
                .map(|member| member.user.id)
                .map(|user_id| (user_id, role_id, name.clone()))
                .collect_vec();
            eyre::Ok(requests)
        })
        .flatten_ok()
        .try_collect()
}
