use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use std::hash::Hash;

pub(crate) struct Gauge<T> {
    data: HashMap<T, u64, FnvBuildHasher>,
}

impl<T: Clone + Eq + Hash> Gauge<T> {
    pub fn new() -> Gauge<T> {
        Gauge {
            data: HashMap::<T, u64, FnvBuildHasher>::default(),
        }
    }

    pub fn update(&mut self, key: T, value: u64) {
        let ivalue = self.data.entry(key).or_insert(0);
        *ivalue = value;
    }

    pub fn values(&self) -> Vec<(T, u64)> { self.data.iter().map(|(k, v)| (k.clone(), *v)).collect() }
}

#[cfg(test)]
mod tests {
    use super::Gauge;

    #[test]
    fn test_gauge_simple_update() {
        let mut gauge = Gauge::new();

        let key = "foo";
        gauge.update(key, 42);

        let values = gauge.values();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0].1, 42);
    }
}
