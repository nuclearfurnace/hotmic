use std::hash::Hash;
use fnv::FnvHashMap;
use super::Sample;

pub struct Gauge<T> {
    data: FnvHashMap<T, u64>,
}

impl<T> Gauge<T>
    where T: Eq + Hash
{
    pub fn new() -> Gauge<T> {
        Gauge { data: FnvHashMap::default() }
    }

    pub fn register(&mut self, key: T) {
        let _ = self.data.entry(key).or_insert(0);
    }

    pub fn deregister(&mut self, key: T) {
        let _ = self.data.remove(&key);
    }

    pub fn update(&mut self, sample: &Sample<T>) {
        match sample {
            Sample::Value(key, value) => {
                if let Some(entry) = self.data.get_mut(&key) {
                    *entry = *value;
                }
            },
            _ => {},
        }
    }

    pub fn value(&self, key: T) -> u64 {
        *self.data.get(&key).unwrap_or(&0)
    }
}
