use crate::{DELETE_BUTTON_ID, TOGGLE_WEEKDAY_BUTTON_ID};
use bot_core::iso_weekday::IsoWeekday;
use chrono::{DateTime, Datelike, Days, Utc, Weekday};
use poise::serenity_prelude::{
    ButtonStyle, Color, CreateActionRow, CreateButton, CreateEmbed, ReactionType,
};
use std::collections::BTreeSet;
use std::iter;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Bedtime {
    pub(crate) first: DateTime<Utc>,
    pub(crate) repeat: BTreeSet<IsoWeekday>,
}

impl Bedtime {
    pub(crate) fn currently_relevant_bedtimes(
        &self,
        now: DateTime<Utc>,
    ) -> BTreeSet<DateTime<Utc>> {
        let today = now.date_naive().and_time(self.first.time()).and_utc();
        let repeats = [today - Days::new(1), today]
            .into_iter()
            .chain((1..=7).map(|offset| today + Days::new(offset)))
            .filter(|&bedtime| {
                bedtime > self.first && self.repeat.contains(&IsoWeekday(bedtime.weekday()))
            });
        iter::once(self.first).chain(repeats).collect()
    }

    pub(crate) fn embed(&self) -> CreateEmbed {
        let now = Utc::now();
        let next = self
            .currently_relevant_bedtimes(now)
            .into_iter()
            .find(|&bedtime| bedtime > now)
            .unwrap_or(self.first);
        CreateEmbed::new()
            .title("Bedtime")
            .description(format!("<t:{}:R>", next.timestamp()))
            .color(Color::DARK_PURPLE)
    }

    pub(crate) fn components(&self) -> Vec<CreateActionRow> {
        vec![
            CreateActionRow::Buttons(vec![
                self.weekday_button(Weekday::Mon),
                self.weekday_button(Weekday::Tue),
                self.weekday_button(Weekday::Wed),
                self.weekday_button(Weekday::Thu),
                self.weekday_button(Weekday::Fri),
            ]),
            CreateActionRow::Buttons(vec![
                self.weekday_button(Weekday::Sat),
                self.weekday_button(Weekday::Sun),
                CreateButton::new(DELETE_BUTTON_ID)
                    .style(ButtonStyle::Danger)
                    .emoji(ReactionType::Unicode("🗑️".to_string())),
            ]),
        ]
    }

    fn weekday_button(&self, weekday: Weekday) -> CreateButton {
        CreateButton::new(format!("{TOGGLE_WEEKDAY_BUTTON_ID}:{}", weekday.num_days_from_monday()))
            .style(if self.repeat.contains(&IsoWeekday(weekday)) {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            })
            .label(weekday.to_string())
    }
}
