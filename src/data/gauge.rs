use super::Sample;
use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use std::hash::Hash;

pub struct Gauge<T> {
    data: HashMap<T, u64, FnvBuildHasher>,
}

impl<T: Eq + Hash> Gauge<T> {
    pub fn new() -> Gauge<T> {
        Gauge {
            data: HashMap::<T, u64, FnvBuildHasher>::default(),
        }
    }

    pub fn register(&mut self, key: T) { let _ = self.data.entry(key).or_insert(0); }

    pub fn deregister(&mut self, key: T) { let _ = self.data.remove(&key); }

    pub fn update(&mut self, sample: &Sample<T>) {
        if let Sample::Value(key, value) = sample {
            if let Some(entry) = self.data.get_mut(&key) {
                *entry = *value;
            }
        }
    }

    pub fn value(&self, key: T) -> u64 { *self.data.get(&key).unwrap_or(&0) }
}

#[cfg(test)]
mod tests {
    use super::Gauge;
    use crate::data::Sample;
    use std::time::Instant;

    #[test]
    fn test_gauge_unregistered_update() {
        let mut gauge = Gauge::new();

        let key = "foo".to_owned();
        let sample = Sample::Value(key.clone(), 42);
        gauge.update(&sample);

        let value = gauge.value(key);
        assert_eq!(value, 0);
    }

    #[test]
    fn test_gauge_simple_update() {
        let mut gauge = Gauge::new();

        let key = "foo".to_owned();
        gauge.register(key.clone());

        let sample = Sample::Value(key.clone(), 42);
        gauge.update(&sample);

        let value = gauge.value(key);
        assert_eq!(value, 42);
    }

    #[test]
    fn test_gauge_sample_support() {
        let mut gauge = Gauge::new();

        // Count samples.
        let ckey = "ckey".to_owned();
        gauge.register(ckey.clone());

        let csample = Sample::Count(ckey.clone(), 42);
        gauge.update(&csample);

        let cvalue = gauge.value(ckey);
        assert_eq!(cvalue, 0);

        // Timing samples.
        let tkey = "tkey".to_owned();
        gauge.register(tkey.clone());

        let tsample = Sample::Timing(tkey.clone(), Instant::now(), Instant::now(), 73);
        gauge.update(&tsample);

        let tvalue = gauge.value(tkey);
        assert_eq!(tvalue, 0);

        // Value samples.
        let vkey = "vkey".to_owned();
        gauge.register(vkey.clone());

        let vsample = Sample::Value(vkey.clone(), 22);
        gauge.update(&vsample);

        let vvalue = gauge.value(vkey);
        assert_eq!(vvalue, 22);
    }
}
