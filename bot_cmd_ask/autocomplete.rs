use crate::ConfigT;
use bot_core::With;
use bot_core::ext::option::OptionExt as _;
use itertools::Itertools;
use poise::serenity_prelude::{AutocompleteChoice, CreateAutocompleteResponse};

pub(crate) async fn existing_game_name<U, E>(ctx: poise::Context<'_, U, E>, input: &str) -> CreateAutocompleteResponse
where
    U: With<ConfigT>,
{
    async {
        let choices = ctx
            .data()
            .with(|c| {
                let guild = ctx.guild().some()?;
                Ok(crate::game_roles(c, &guild)
                    .into_iter()
                    .filter(|role| {
                        role.name.to_lowercase().trim().starts_with(input)
                            || c.games.get(&role.id).unwrap().title_pattern.0.is_match(&input).is_ok_and(|x| x)
                    })
                    .map(|role| role.name)
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
