use std::hash::Hash;
use fnv::FnvHashMap;
use super::Sample;

pub struct Counter<T> {
    pub data: FnvHashMap<T, i64>,
}

impl<T> Counter<T>
    where T: Eq + Hash
{
    pub fn new() -> Counter<T> {
        Counter { data: FnvHashMap::default() }
    }

    pub fn register(&mut self, key: T) {
        let _ = self.data.entry(key).or_insert(0);
    }

    pub fn deregister(&mut self, key: T) {
        let _ = self.data.remove(&key);
    }

    pub fn update(&mut self, sample: &Sample<T>) {
        match sample {
            Sample::Timing(key, _, _, count) => {
                let coerced = *count as i64;
                if let Some(entry) = self.data.get_mut(&key) {
                    *entry += coerced;
                }
            },
            Sample::Count(key, count) => {
                if let Some(entry) = self.data.get_mut(&key) {
                    *entry += *count;
                }
            },
            Sample::Value(key, _) => {
                if let Some(entry) = self.data.get_mut(&key) {
                    *entry += 1;
                }
            },
        }
    }

    pub fn value(&self, key: T) -> i64 {
        *self.data.get(&key).unwrap_or(&0)
    }
}
