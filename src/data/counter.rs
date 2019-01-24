use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use std::hash::Hash;

pub(crate) struct Counter<T> {
    data: HashMap<T, i64, FnvBuildHasher>,
}

impl<T: Clone + Eq + Hash> Counter<T> {
    pub fn new() -> Counter<T> {
        Counter {
            data: HashMap::<T, i64, FnvBuildHasher>::default(),
        }
    }

    pub fn update(&mut self, key: T, delta: i64) {
        let value = self.data.entry(key).or_insert(0);
        *value += delta;
    }

    pub fn values(&self) -> Vec<(T, i64)> {
        self.data.iter().map(|(k, v)| (k.clone(), *v)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Counter;

    #[test]
    fn test_counter_simple_update() {
        let mut counter = Counter::new();

        let key = "foo";
        counter.update(key.clone(), 42);

        let values = counter.values();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].1, 42);
    }
}
