use crate::{DELETE_BUTTON_ID, TOGGLE_WEEKDAY_BUTTON_ID};
use bot_core::iso_weekday::IsoWeekday;
use chrono::{DateTime, Datelike, Days, Utc, Weekday};
use poise::serenity_prelude::{
    ButtonStyle, Color, CreateActionRow, CreateButton, CreateEmbed, ReactionType, UserId,
};
use std::collections::BTreeSet;
use std::iter;
use uuid::Uuid;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Bedtime {
    pub(crate) user: UserId,
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
            .title("🌙 Bedtime")
            .description(format!("<t:{}:R>", next.timestamp()))
            .color(Color::DARK_PURPLE)
    }

    pub(crate) fn components(&self, id: Uuid) -> Vec<CreateActionRow> {
        vec![
            CreateActionRow::Buttons(vec![
                self.weekday_button(id, Weekday::Mon),
                self.weekday_button(id, Weekday::Tue),
                self.weekday_button(id, Weekday::Wed),
                self.weekday_button(id, Weekday::Thu),
                self.weekday_button(id, Weekday::Fri),
            ]),
            CreateActionRow::Buttons(vec![
                self.weekday_button(id, Weekday::Sat),
                self.weekday_button(id, Weekday::Sun),
                CreateButton::new(format!("{DELETE_BUTTON_ID}:{id}"))
                    .style(ButtonStyle::Danger)
                    .emoji(ReactionType::Unicode("🗑️".to_string())),
            ]),
        ]
    }

    fn weekday_button(&self, id: Uuid, weekday: Weekday) -> CreateButton {
        let weekday_str = weekday.to_string();
        CreateButton::new(format!("{TOGGLE_WEEKDAY_BUTTON_ID}:{id}:{weekday_str}"))
            .style(if self.repeat.contains(&IsoWeekday(weekday)) {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            })
            .label(weekday_str)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::{TimeDelta, TimeZone};

    #[test]
    fn currently_relevant_bedtimes_when_after_first() {
        let today = Utc.with_ymd_and_hms(2025, 1, 1, 1, 15, 0).unwrap();
        let bedtime2 = Bedtime {
            user: Default::default(),
            first: today - TimeDelta::days(10),
            repeat: [Weekday::Tue, Weekday::Wed, Weekday::Sun]
                .into_iter()
                .map(IsoWeekday)
                .collect(),
        };
        assert_eq!(
            bedtime2.currently_relevant_bedtimes(today - TimeDelta::minutes(1)),
            [
                Utc.with_ymd_and_hms(2024, 12, 22, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 12, 31, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 1, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 5, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 7, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 8, 1, 15, 0).unwrap()
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
        );
    }

    #[test]
    fn currently_relevant_bedtimes_when_before_first() {
        let today = Utc.with_ymd_and_hms(2025, 1, 1, 1, 15, 0).unwrap();
        let bedtime = Bedtime {
            user: Default::default(),
            first: today,
            repeat: [Weekday::Tue, Weekday::Wed, Weekday::Sun]
                .into_iter()
                .map(IsoWeekday)
                .collect(),
        };
        assert_eq!(
            bedtime.currently_relevant_bedtimes(today - TimeDelta::minutes(1)),
            [
                Utc.with_ymd_and_hms(2025, 1, 1, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 5, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 7, 1, 15, 0).unwrap(),
                Utc.with_ymd_and_hms(2025, 1, 8, 1, 15, 0).unwrap()
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
        );
    }
}
