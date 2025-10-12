use self::weekly_time_repr::WeeklyTimeRepr;
use chrono::{DateTime, Utc};

#[derive(serde::Serialize, serde::Deserialize, Copy, Clone, Debug)]
pub(crate) struct WeeklyTime(DateTime<Utc>);

impl PartialEq for WeeklyTime {
    fn eq(&self, other: &Self) -> bool {
        WeeklyTimeRepr::from(self) == WeeklyTimeRepr::from(other)
    }
}

impl Eq for WeeklyTime {}

impl PartialOrd for WeeklyTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WeeklyTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        WeeklyTimeRepr::from(self).cmp(&WeeklyTimeRepr::from(other))
    }
}

mod weekly_time_repr {
    use chrono::{Datelike as _, NaiveTime, Weekday};

    #[derive(serde::Serialize, serde::Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
    pub(super) struct WeeklyTimeRepr {
        weekday: Weekday,
        time: NaiveTime,
    }

    impl PartialOrd for WeeklyTimeRepr {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for WeeklyTimeRepr {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.weekday
                .number_from_monday()
                .cmp(&other.weekday.num_days_from_monday())
                .then_with(|| self.time.cmp(&other.time))
        }
    }

    impl From<&super::WeeklyTime> for WeeklyTimeRepr {
        fn from(value: &super::WeeklyTime) -> Self {
            WeeklyTimeRepr { weekday: value.0.weekday(), time: value.0.time() }
        }
    }
}
