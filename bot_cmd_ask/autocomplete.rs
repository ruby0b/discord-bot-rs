use crate::ConfigT;
use bot_core::With;
use bot_core::ext::option::OptionExt as _;
use itertools::Itertools;
use poise::serenity_prelude::{AutocompleteChoice, CreateAutocompleteResponse};

pub async fn existing_game_name<U, E>(ctx: poise::Context<'_, U, E>, input: &str) -> CreateAutocompleteResponse
where
    U: With<ConfigT>,
{
    async {
        let choices = ctx
            .data()
            .with(|c| {
                let guild = ctx.guild().some()?;
                Ok(c.games
                    .keys()
                    .filter_map(|&role_id| guild.roles.get(&role_id).map(|r| r.name.clone()))
                    .filter(|name| name.to_lowercase().trim().starts_with(input))
                    .map(|name| AutocompleteChoice::new(name.clone(), name))
                    .take(25)
                    .collect_vec())
            })
            .await?;
        eyre::Ok(CreateAutocompleteResponse::new().set_choices(choices))
    }
    .await
    .inspect_err(|e| tracing::error!("Failed to auto-complete game names: {e:?}"))
    .unwrap_or_default()
}
