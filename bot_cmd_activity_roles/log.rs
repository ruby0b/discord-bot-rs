use chrono::prelude::Weekday;

/// Daily activity is 1 bit, each week is a byte \<MO TU WE TH FR SA SU XX> (we ignore the last one)
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Log(#[serde(with = "bot_core::serde::hex_bytes")] Vec<u8>);

impl Default for Log {
    fn default() -> Self {
        Self(vec![0u8])
    }
}

impl Log {
    pub(crate) fn logged_days(&self) -> usize {
        self.0.iter().map(|&week| week.count_ones() as usize).sum()
    }

    pub(crate) fn set_today(&mut self, today: Weekday) {
        self.0[0] |= 0b10000000 >> today.num_days_from_monday();
    }

    pub(crate) fn get_today(&self, today: Weekday) -> bool {
        (self.0[0] & (0b10000000 >> today.num_days_from_monday())) != 0
    }

    pub(crate) fn start_new_week(&mut self, max_weeks: usize) {
        self.0.insert(0, 0u8);
        self.0.resize(max_weeks, 0u8);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn day_get_correct() {
        let mut log = Log(vec![0b01000010, 0b10000100]);
        assert!(!log.get_today(Weekday::Mon));
        assert!(log.get_today(Weekday::Tue));
        assert!(!log.get_today(Weekday::Wed));
        assert!(!log.get_today(Weekday::Thu));
        assert!(!log.get_today(Weekday::Fri));
        assert!(!log.get_today(Weekday::Sat));
        assert!(log.get_today(Weekday::Sun));
        assert_eq!(log.logged_days(), 4);

        log.set_today(Weekday::Tue); // no-op
        log.set_today(Weekday::Fri);

        assert!(!log.get_today(Weekday::Mon));
        assert!(log.get_today(Weekday::Tue));
        assert!(!log.get_today(Weekday::Wed));
        assert!(!log.get_today(Weekday::Thu));
        assert!(log.get_today(Weekday::Fri));
        assert!(!log.get_today(Weekday::Sat));
        assert!(log.get_today(Weekday::Sun));
        assert_eq!(log.logged_days(), 5);
    }
}
