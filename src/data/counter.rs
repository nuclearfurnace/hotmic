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

#[cfg(test)]
mod tests {
    use std::time::Instant;
    use super::Counter;
    use data::Sample;

    #[test]
    fn test_counter_unregistered_update() {
        let mut counter = Counter::new();

        let key = "foo".to_owned();
        let sample = Sample::Count(key.clone(), 42);
        counter.update(&sample);

        let value = counter.value(key);
        assert_eq!(value, 0);
    }

    #[test]
    fn test_counter_simple_update() {
        let mut counter = Counter::new();

        let key = "foo".to_owned();
        counter.register(key.clone());

        let sample = Sample::Count(key.clone(), 42);
        counter.update(&sample);

        let value = counter.value(key);
        assert_eq!(value, 42);
    }

    #[test]
    fn test_counter_sample_support() {
        let mut counter = Counter::new();

        // Count samples.
        let ckey = "ckey".to_owned();
        counter.register(ckey.clone());

        let csample = Sample::Count(ckey.clone(), 42);
        counter.update(&csample);

        let cvalue = counter.value(ckey);
        assert_eq!(cvalue, 42);

        // Timing samples.
        let tkey = "tkey".to_owned();
        counter.register(tkey.clone());

        let tsample = Sample::Timing(tkey.clone(), Instant::now(), Instant::now(), 73);
        counter.update(&tsample);

        let tvalue = counter.value(tkey);
        assert_eq!(tvalue, 73);

        // Value samples.
        let vkey = "vkey".to_owned();
        counter.register(vkey.clone());

        let vsample = Sample::Value(vkey.clone(), 22);
        counter.update(&vsample);

        let vvalue = counter.value(vkey);
        assert_eq!(vvalue, 1);
    }
}
