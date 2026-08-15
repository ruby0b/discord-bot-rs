use chrono::prelude::{DateTime, TimeZone};

pub mod iso_week;
pub mod iso_weekday;

pub fn discord_timestamp<Tz: TimeZone>(time: DateTime<Tz>) -> String {
    format!("<t:{}:R>", time.timestamp())
}
