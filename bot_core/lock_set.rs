use dashmap::DashMap;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct LockSet<K: Eq + Hash>(DashMap<K, Arc<Mutex<()>>>);

impl<K: Eq + Hash> LockSet<K> {
    pub fn get(&self, key: K) -> Arc<Mutex<()>> {
        self.0.entry(key).or_default().clone()
    }
}
