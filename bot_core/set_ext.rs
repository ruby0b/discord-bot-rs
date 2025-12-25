use std::collections::BTreeSet;

pub enum ToggleResult {
    Removed,
    Inserted,
}

pub trait BTreeSetExt<T> {
    fn toggle(&mut self, key: T) -> ToggleResult;
}

impl<T> BTreeSetExt<T> for BTreeSet<T>
where
    T: Ord,
{
    fn toggle(&mut self, key: T) -> ToggleResult {
        if self.remove(&key) {
            ToggleResult::Removed
        } else {
            self.insert(key);
            ToggleResult::Inserted
        }
    }
}
