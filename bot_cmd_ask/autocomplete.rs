use crate::ConfigT;
use bot_core::With;
use itertools::Itertools;
use poise::serenity_prelude::{AutocompleteChoice, CreateAutocompleteResponse};

pub async fn existing_game_name<U, E>(ctx: poise::Context<'_, U, E>, input: &str) -> CreateAutocompleteResponse
where
    U: With<ConfigT>,
{
    async {
        let matching_game_names = ctx
            .data()
            .with_ok(|c| {
                c.games.keys().filter(|name| name.to_lowercase().trim().starts_with(input)).cloned().collect_vec()
            })
            .await?;
        eyre::Ok(CreateAutocompleteResponse::new().set_choices(
            matching_game_names.into_iter().map(|name| AutocompleteChoice::new(name.clone(), name)).take(25).collect(),
        ))
    }
    .await
    .inspect_err(|e| tracing::error!("Failed to auto-complete game names: {e:?}"))
    .unwrap_or_default()
}
