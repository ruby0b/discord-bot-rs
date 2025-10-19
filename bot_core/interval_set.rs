use std::ops::Range;

#[derive(Debug, Default, Clone)]
pub struct IntervalSet<T: Ord + Copy> {
    intervals: Vec<Range<T>>,
}

impl<T: Ord + Copy> IntervalSet<T> {
    pub fn new() -> Self {
        Self { intervals: Vec::new() }
    }

    pub fn add(&mut self, start: T, end: T) {
        self.intervals.push(start..end);
        self.normalize();
    }

    pub fn find(&self, time: T) -> Option<Range<T>> {
        self.intervals.iter().find(|r| r.contains(&time)).cloned()
    }

    fn normalize(&mut self) {
        if self.intervals.is_empty() {
            return;
        }

        self.intervals.sort_by_key(|r| r.start);
        let mut merged = vec![self.intervals[0].clone()];

        for r in &self.intervals[1..] {
            let last = merged.last_mut().unwrap();
            if r.start > last.end {
                // disjoint
                merged.push(r.clone());
            } else if r.end > last.end {
                // merge
                last.end = r.end;
            }
        }

        self.intervals = merged;
    }
}

impl<T: Ord + Copy> FromIterator<Range<T>> for IntervalSet<T> {
    fn from_iter<I: IntoIterator<Item = Range<T>>>(iter: I) -> Self {
        let mut c = IntervalSet::new();
        for i in iter {
            c.add(i.start, i.end);
        }
        c
    }
}
