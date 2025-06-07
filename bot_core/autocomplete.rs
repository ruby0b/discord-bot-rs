use chrono::Timelike;
use itertools::Itertools as _;
use poise::serenity_prelude::all::{AutocompleteChoice, CreateAutocompleteResponse};

pub async fn voice_region<U, E>(
    ctx: poise::Context<'_, U, E>,
    _input: &str,
) -> CreateAutocompleteResponse {
    async {
        let regions = ctx.http().get_guild_regions(ctx.guild_id().unwrap()).await?;
        eyre::Ok(CreateAutocompleteResponse::new().set_choices(
            regions.into_iter().map(|r| AutocompleteChoice::new(r.name, r.id)).take(25).collect(),
        ))
    }
    .await
    .inspect_err(|e| tracing::error!("Failed to auto-complete guild regions: {e:?}"))
    .unwrap_or_default()
}

pub async fn time<U, E>(_ctx: poise::Context<'_, U, E>, input: &str) -> CreateAutocompleteResponse {
    let mut choices: Vec<_> = (0..=23).cartesian_product([0, 15, 30, 45]).collect();

    let now = chrono::Local::now();
    choices.rotate_left(now.hour() as usize * 4 + now.minute() as usize / 15 + 1);

    let choices = choices
        .into_iter()
        .map(|(h, m)| format!("{h:02}:{m:02}"))
        .filter(|time| time.starts_with(input))
        .map(|time| AutocompleteChoice::new(time.clone(), time))
        .take(25)
        .collect();

    CreateAutocompleteResponse::new().set_choices(choices)
}
