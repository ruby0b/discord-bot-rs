use chrono::Weekday;

#[derive(serde::Serialize, serde::Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct IsoWeekday(pub Weekday);

impl PartialOrd for IsoWeekday {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IsoWeekday {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.number_from_monday().cmp(&other.0.num_days_from_monday())
    }
}
