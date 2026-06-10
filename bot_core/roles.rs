use crate::ext::option::OptionExt;
use crate::safe_name;
use eyre::Result;
use poise::serenity_prelude::{Context, GuildId, RoleId, UserId};
use std::collections::{HashMap, HashSet};

pub async fn enforce_roles(
    ctx: &Context,
    guild_id: GuildId,
    expected_roles: &HashMap<RoleId, HashSet<UserId>>,
    mut request_budget: u16,
) -> Result<()> {
    let actual_roles = {
        let guild = ctx.cache.guild(guild_id).some()?;
        let mut actual_roles: HashMap<RoleId, HashSet<UserId>> = HashMap::new();
        for member in guild.members.values() {
            for &role_id in &member.roles {
                actual_roles.entry(role_id).or_default().insert(member.user.id);
            }
        }
        actual_roles
    };

    for (&role_id, expected_users) in expected_roles {
        let actual_users = match actual_roles.get(&role_id) {
            Some(x) => x,
            None => &HashSet::new(),
        };

        for &user_id in actual_users.difference(expected_users) {
            if request_budget == 0 {
                tracing::warn!("Ran out of request budget");
                return Ok(());
            }
            tracing::info!("Removing role {role_id} from member {}", safe_name(ctx, user_id));
            ctx.http.remove_member_role(guild_id, user_id, role_id, None).await?;
            request_budget -= 1;
        }

        for &user_id in expected_users.difference(actual_users) {
            if request_budget == 0 {
                tracing::warn!("Ran out of request budget");
                return Ok(());
            }
            tracing::info!("Adding role {role_id} to member {}", safe_name(ctx, user_id));
            ctx.http.add_member_role(guild_id, user_id, role_id, None).await?;
            request_budget -= 1;
        }
    }
    Ok(())
}
