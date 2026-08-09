use crate::ConfigT;
use bot_core::ext::option::OptionExt;
use bot_core::roles::enforce_roles;
use bot_core::{State, With};
use eyre::Result;
use poise::serenity_prelude::{Context, GuildId, RoleId, UserId};
use std::collections::{HashMap, HashSet};
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
            _ = sleep(Duration::from_secs(60)) => {
                tracing::trace!("Periodic update");
                update(&ctx, &data).await
            }
        } {
            tracing::error!("Error in worker: {error:?}");
        }
    }
}

const ROLE_ADD_REMOVE_PER_MINUTE: u16 = 20;

async fn update(ctx: &Context, data: &(impl With<ConfigT> + State<GuildId>)) -> Result<()> {
    let guild_id: GuildId = *data.state();
    let games = data.with_ok(|c| c.games.clone()).await?;

    let role_ids: HashSet<RoleId> = {
        let guild = ctx.cache.guild(guild_id).some()?;
        guild.roles.keys().copied().collect()
    };

    let mut enforced_roles: HashMap<RoleId, HashSet<UserId>> = HashMap::new();
    {
        let guild = ctx.cache.guild(guild_id).unwrap();
        for (&role_id, game) in &games {
            if !role_ids.contains(&role_id) {
                tracing::warn!("Game role {role_id} does not exist");
                continue;
            }

            // insert users that should have the game role
            let mut users = HashSet::new();
            for member in guild.members.values() {
                if role_ids.contains(&game.parent_role)
                    && member.roles.contains(&game.parent_role)
                    && !game.opted_out_users.contains(&member.user.id)
                {
                    users.insert(member.user.id);
                }
            }

            enforced_roles.insert(role_id, users);
        }
    }

    enforce_roles(ctx, guild_id, &enforced_roles, ROLE_ADD_REMOVE_PER_MINUTE).await?;

    Ok(())
}
