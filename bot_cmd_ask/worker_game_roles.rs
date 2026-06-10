use crate::{ConfigT, Game};
use bot_core::ext::option::OptionExt;
use bot_core::roles::enforce_roles;
use bot_core::{State, With};
use eyre::Result;
use poise::serenity_prelude::{Builder as _, Context, EditRole, GuildId, Permissions, RoleId, UserId};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc;
use tokio::time::sleep;

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
            _ = sleep(Duration::from_secs(30)) => {
                tracing::trace!("Periodic update");
                update(&ctx, &data).await
            }
        } {
            tracing::error!("Error in worker: {error:?}");
        }
    }
}

const ROLE_CREATIONS_PER_MINUTE: u16 = 3;
const ROLE_ADD_REMOVE_PER_MINUTE: u16 = 20;

async fn update(ctx: &Context, data: &(impl With<ConfigT> + State<GuildId>)) -> Result<()> {
    let guild_id: GuildId = *data.state();
    let games = data.with_ok(|c| c.games.clone()).await?;

    let role_ids: HashSet<RoleId> = {
        let guild = ctx.cache.guild(guild_id).some()?;
        guild.roles.keys().copied().collect()
    };
    let roles_by_name: HashMap<String, RoleId> = {
        let guild = ctx.cache.guild(guild_id).some()?;
        guild.roles.iter().map(|(&id, role)| (role.name.clone(), id)).collect()
    };
    let mut existing_game_roles: BTreeMap<String, RoleId> =
        games.keys().filter_map(|name| roles_by_name.get(name).map(|&id| (name.clone(), id))).collect();

    // only do a few role creations at a time to not send too many requests at once
    let create_role_requests = build_create_role_requests(ctx, guild_id, &games, &existing_game_roles)?;
    for (name, create_role) in create_role_requests.into_iter().take(ROLE_CREATIONS_PER_MINUTE as usize) {
        tracing::info!("Creating game role {create_role:?}");
        let game_role = create_role.execute(ctx, (guild_id, None)).await?;
        existing_game_roles.insert(name, game_role.id);
    }

    let mut enforced_roles: HashMap<RoleId, HashSet<UserId>> = HashMap::new();
    {
        let guild = ctx.cache.guild(guild_id).unwrap();
        for (name, game) in &games {
            let mut users = HashSet::new();
            // insert users that should have the game role
            for member in guild.members.values() {
                if role_ids.contains(&game.parent_role)
                    && member.roles.contains(&game.parent_role)
                    && !game.opted_out_users.contains(&member.user.id)
                {
                    users.insert(member.user.id);
                }
            }
            enforced_roles.insert(*existing_game_roles.get(name).unwrap(), users);
        }
    }
    enforce_roles(ctx, guild_id, &enforced_roles, ROLE_ADD_REMOVE_PER_MINUTE).await?;

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
            let builder = EditRole::new().name(name.clone()).permissions(Permissions::empty());
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
