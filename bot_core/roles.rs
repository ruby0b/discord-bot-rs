use crate::ext::option::OptionExt;
use crate::safe_name;
use eyre::Result;
use poise::serenity_prelude::{Context, GuildId, RoleId, UserId};
use std::collections::{HashMap, HashSet};
use tracing::{debug, info, warn};

pub async fn enforce_roles(
    ctx: &Context,
    guild_id: GuildId,
    expected_roles: &HashMap<RoleId, HashSet<UserId>>,
    mut request_budget: u16,
) -> Result<()> {
    let (member_ids, actual_roles) = {
        let guild = ctx.cache.guild(guild_id).some()?;
        let mut member_ids: HashSet<UserId> = HashSet::new();
        let mut actual_roles: HashMap<RoleId, HashSet<UserId>> = HashMap::new();
        for member in guild.members.values() {
            member_ids.insert(member.user.id);
            for &role_id in &member.roles {
                actual_roles.entry(role_id).or_default().insert(member.user.id);
            }
        }
        (member_ids, actual_roles)
    };

    for (&role_id, expected_users) in expected_roles {
        let actual_users = match actual_roles.get(&role_id) {
            Some(x) => x,
            None => &HashSet::new(),
        };

        for &user_id in actual_users.difference(expected_users) {
            if request_budget == 0 {
                warn!("Ran out of request budget");
                return Ok(());
            }
            info!("Removing role {role_id} from member {}", safe_name(ctx, user_id));
            ctx.http.remove_member_role(guild_id, user_id, role_id, None).await?;
            request_budget -= 1;
        }

        for &user_id in expected_users.difference(actual_users) {
            if !member_ids.contains(&user_id) {
                debug!("Ignoring user id that's not a member: {user_id}");
                continue;
            }
            if request_budget == 0 {
                warn!("Ran out of request budget");
                return Ok(());
            }
            info!("Adding role {role_id} to member {}", safe_name(ctx, user_id));
            ctx.http.add_member_role(guild_id, user_id, role_id, None).await?;
            request_budget -= 1;
        }
    }
    Ok(())
}
