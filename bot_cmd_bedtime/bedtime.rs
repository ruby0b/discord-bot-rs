use crate::{ConfigT, DELETE_BUTTON_ID, SELECT_BEDTIME_ID, TOGGLE_WEEKDAY_BUTTON_ID};
use bot_core::With;
use bot_core::iso_weekday::IsoWeekday;
use chrono::{DateTime, Datelike, Days, Local, TimeDelta, TimeZone, Utc, Weekday};
use eyre::Result;
use itertools::Itertools as _;
use poise::CreateReply;
use poise::serenity_prelude::{
    ButtonStyle, Color, CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenu,
    CreateSelectMenuKind, CreateSelectMenuOption, ReactionType, UserId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
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
            .filter(|&bedtime| bedtime > self.first)
            .filter(|&bedtime| self.repeat.contains(&IsoWeekday(bedtime.weekday())));
        iter::once(self.first).chain(repeats).collect()
    }

    pub(crate) fn next(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        self.currently_relevant_bedtimes(now)
            .into_iter()
            .find(|&bedtime| bedtime > now)
            .unwrap_or(self.first)
    }

    pub(crate) async fn reply(
        &self,
        id: Uuid,
        data: &impl With<ConfigT>,
        now: DateTime<Utc>,
    ) -> Result<CreateReply> {
        Ok(CreateReply::new()
            .embed(self.embed(now))
            .components(self.components(id, data, now).await?))
    }

    pub(crate) fn embed(&self, now: DateTime<Utc>) -> CreateEmbed {
        CreateEmbed::new()
            .title(format!("🌙 Bedtime – {}", format_datetime(self.next(now), now, &Local)))
            .description(format!("<t:{}:R>", self.next(Utc::now()).timestamp()))
            .color(Color::DARK_PURPLE)
    }

    pub(crate) async fn components(
        &self,
        id: Uuid,
        data: &impl With<ConfigT>,
        now: DateTime<Utc>,
    ) -> Result<Vec<CreateActionRow>> {
        let mut components = self.select_menu_component(id, data, now).await?;

        components.push(CreateActionRow::Buttons(vec![
            self.weekday_button(id, Weekday::Mon),
            self.weekday_button(id, Weekday::Tue),
            self.weekday_button(id, Weekday::Wed),
            self.weekday_button(id, Weekday::Thu),
            self.weekday_button(id, Weekday::Fri),
        ]));

        components.push(CreateActionRow::Buttons(vec![
            self.weekday_button(id, Weekday::Sat),
            self.weekday_button(id, Weekday::Sun),
            CreateButton::new(format!("{DELETE_BUTTON_ID}:{id}"))
                .style(ButtonStyle::Danger)
                .emoji(ReactionType::Unicode("🗑️".to_string())),
        ]));

        Ok(components)
    }

    pub(crate) async fn select_menu_component(
        &self,
        id: Uuid,
        data: &impl With<ConfigT>,
        now: DateTime<Utc>,
    ) -> Result<Vec<CreateActionRow>> {
        let mut components = vec![];

        // add a selection menu to view other bedtimes
        let other_bedtimes: BTreeSet<_> = all_bedtimes(data, self.user)
            .await?
            .into_iter()
            .filter(|(other_id, _)| *other_id != id)
            .collect();
        if !other_bedtimes.is_empty() {
            let options = other_bedtimes
                .into_iter()
                .map(|(other_id, bedtime)| {
                    CreateSelectMenuOption::new(
                        format_datetime(bedtime.next(now), now, &Local),
                        other_id,
                    )
                    .description(if !bedtime.repeat.is_empty() {
                        let repeats = bedtime.repeat.iter().map(|wd| wd.0.to_string()).join(", ");
                        format!("Repeats on: {repeats}")
                    } else {
                        "Never repeats".to_string()
                    })
                })
                .collect_vec();

            components.push(CreateActionRow::SelectMenu(
                CreateSelectMenu::new(SELECT_BEDTIME_ID, CreateSelectMenuKind::String { options })
                    .min_values(1)
                    .max_values(1)
                    .placeholder("View other bedtimes..."),
            ));
        }

        Ok(components)
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

async fn all_bedtimes(
    data: &impl With<ConfigT>,
    user_id: UserId,
) -> Result<BTreeMap<Uuid, Bedtime>> {
    data.with_ok(|cfg| {
        cfg.bedtimes
            .iter()
            .filter(|(_, b)| b.user == user_id)
            .map(|(id, b)| (*id, b.clone()))
            .collect()
    })
    .await
}

fn format_datetime<TZ>(dt: DateTime<Utc>, now: DateTime<Utc>, display_tz: &TZ) -> String
where
    TZ: TimeZone,
    TZ::Offset: Display,
{
    dt.with_timezone(display_tz)
        .format(
            if dt.num_days_from_ce() == now.num_days_from_ce() || dt < now + TimeDelta::hours(12) {
                "%H:%M"
            } else if dt.num_days_from_ce() - now.num_days_from_ce() == 1 {
                "Tomorrow at %H:%M"
            } else if dt < now + TimeDelta::weeks(1) {
                "%A at %H:%M"
            } else if dt.year() == now.year() {
                "%A, %d.%m. at %H:%M"
            } else {
                "%A, %d.%m.%Y at %H:%M"
            },
        )
        .to_string()
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
