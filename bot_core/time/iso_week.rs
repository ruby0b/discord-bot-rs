use chrono::IsoWeek;

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Hash)]
pub struct IsoWeekSerde(#[serde(with = "iso_week_serde")] pub IsoWeek);

mod iso_week_serde {
    use chrono::{Datelike, IsoWeek, NaiveDate, Weekday};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(w: &IsoWeek, s: S) -> Result<S::Ok, S::Error> {
        format!("{w:?}").serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<IsoWeek, D::Error> {
        let s = String::deserialize(d)?;
        let (year, week) = s.split_once("-W").ok_or_else(|| serde::de::Error::custom("expected 1234-W12"))?;
        let year: i32 = year.parse().map_err(serde::de::Error::custom)?;
        let week: u32 = week.parse().map_err(serde::de::Error::custom)?;
        NaiveDate::from_isoywd_opt(year, week, Weekday::Mon)
            .map(|d| d.iso_week())
            .ok_or_else(|| serde::de::Error::custom("invalid iso week"))
    }
}
