use bot_core::{EvtContext, With};
use eyre::Result;
use poise::serenity_prelude::{
    Builder, CreateAttachment, EditRole, Message, RoleId, UserId, parse_emoji,
};
use rand::Rng;
use std::collections::BTreeMap;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ConfigT {
    by_role: BTreeMap<RoleId, RoleConfig>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
struct RoleConfig {
    chance: f64,
    user_chances: BTreeMap<UserId, f64>,
}

pub async fn message<D: With<ConfigT>>(ctx: EvtContext<'_, D>, message: &Message) -> Result<()> {
    if message.author.bot {
        return Ok(());
    }

    let Some(guild_id) = message.guild_id else { return Ok(()) };

    let roles = ctx
        .user_data
        .with_ok(|cfg| {
            cfg.by_role
                .iter()
                .map(|(r, c)| (*r, *c.user_chances.get(&message.author.id).unwrap_or(&c.chance)))
                .collect::<Vec<_>>()
        })
        .await?
        .into_iter()
        .filter(|(_, c)| rand::rng().random_bool(*c))
        .map(|(r, _)| r)
        .collect::<Vec<_>>();

    if roles.is_empty() {
        return Ok(());
    }

    for word in message.content.split_ascii_whitespace() {
        let edit_role = {
            if let Some(emoji) = parse_emoji(word) {
                let icon = CreateAttachment::url(ctx.serenity_context, &emoji.url()).await?;
                EditRole::default().icon(Some(&icon))
            } else if emojis::get(word).is_some() {
                EditRole::default().unicode_emoji(Some(word.to_string()))
            } else {
                continue;
            }
        };

        for role_id in roles {
            edit_role.clone().execute(ctx.serenity_context, (guild_id, Some(role_id))).await?;
        }

        break;
    }

    Ok(())
}
