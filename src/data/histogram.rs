use std::hash::Hash;
use fnv::FnvHashMap;
use hdrhistogram::Histogram as HdrHistogram;
use std::time::{Instant, Duration};
use super::Sample;
use helper::duration_as_nanos;

pub struct Histogram<T> {
    window: Duration,
    granularity: Duration,
    data: FnvHashMap<T, WindowedHistogram>,
}

impl<T> Histogram<T>
    where T: Eq + Hash
{
    pub fn new(window: Duration, granularity: Duration) -> Histogram<T> {
        Histogram {
            window: window,
            granularity: granularity,
            data: FnvHashMap::default(),
        }
    }

    pub fn register(&mut self, key: T) {
        let _ = self.data.entry(key).or_insert(WindowedHistogram::new(
            self.window.clone(), self.granularity.clone()
        ));
    }

    pub fn deregister(&mut self, key: T) {
        let _ = self.data.remove(&key);
    }

    pub fn update(&mut self, sample: &Sample<T>) {
        match sample {
            Sample::Timing(key, start, end, _) => {
                if let Some(entry) = self.data.get_mut(&key) {
                    let delta = *end - *start;
                    let delta = (delta.as_secs() * 1_000_000_000) + delta.subsec_nanos() as u64;
                    entry.update(delta);
                }
            },
            Sample::Value(key, value) => {
                if let Some(entry) = self.data.get_mut(&key) {
                    entry.update(*value);
                }
            },
            _ => {},
        }
    }

    pub fn upkeep(&mut self, at: Instant) {
        for (_, histogram) in self.data.iter_mut() {
            histogram.upkeep(at);
        }
    }

    pub fn snapshot(&self, key: T) -> Option<HdrHistogram<u64>> {
        match self.data.get(&key) {
            Some(wh) => Some(wh.merged()),
            _ => None,
        }
    }
}

pub struct WindowedHistogram {
    buckets: Vec<HdrHistogram<u64>>,
    num_buckets: usize,
    bucket_index: usize,
    last_upkeep: Instant,
    granularity: Duration,
}

impl WindowedHistogram {
    pub fn new(window: Duration, granularity: Duration) -> WindowedHistogram {
        let num_buckets = ((duration_as_nanos(window) / duration_as_nanos(granularity)) as usize) + 1;
        let mut buckets = Vec::with_capacity(num_buckets);

        for _ in 0..num_buckets {
            let histogram = HdrHistogram::new_with_bounds(1, u64::max_value(), 3).unwrap();
            buckets.push(histogram);
        }

        WindowedHistogram {
            buckets: buckets,
            num_buckets: num_buckets,
            bucket_index: 0,
            last_upkeep: Instant::now(),
            granularity: granularity,
        }
    }

    pub fn upkeep(&mut self, at: Instant) {
        if at >= self.last_upkeep + self.granularity {
            self.bucket_index += 1;
            self.bucket_index %= self.num_buckets;
            self.buckets[self.bucket_index].clear();
            self.last_upkeep = at;
        }
    }

    pub fn update(&mut self, value: u64) {
        self.buckets[self.bucket_index].saturating_record(value);
    }

    pub fn merged(&self) -> HdrHistogram<u64> {
        let mut base = HdrHistogram::new_from(&self.buckets[self.bucket_index]);
        for histogram in &self.buckets {
            base.add(histogram).unwrap()
        }

        base
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Instant, Duration};
    use super::{Histogram, WindowedHistogram};
    use data::Sample;

    #[test]
    fn test_histogram_unregistered_update() {
        let mut histogram = Histogram::new(Duration::new(5, 0), Duration::new(1, 0));

        let key = "foo".to_owned();
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(100);
        let sample = Sample::Timing(key.clone(), t0, t1, 1);
        histogram.update(&sample);

        let value = histogram.snapshot(key);
        assert!(value.is_none());
    }

    #[test]
    fn test_histogram_simple_update() {
        let mut histogram = Histogram::new(Duration::new(5, 0), Duration::new(1, 0));

        let key = "foo".to_owned();
        histogram.register(key.clone());

        let t0 = Instant::now();
        let t1 = t0 + Duration::from_nanos(1245);
        let sample = Sample::Timing(key.clone(), t0, t1, 1);
        histogram.update(&sample);

        let value = histogram.snapshot(key);
        assert!(value.is_some());

        let hdr = value.unwrap();
        assert_eq!(hdr.len(), 1);
        assert_eq!(hdr.max(), 1245);
    }

    #[test]
    fn test_histogram_sample_support() {
        let mut histogram = Histogram::new(Duration::new(5, 0), Duration::new(1, 0));

        // Count samples.
        let ckey = "ckey".to_owned();
        histogram.register(ckey.clone());

        let csample = Sample::Count(ckey.clone(), 42);
        histogram.update(&csample);

        let cvalue = histogram.snapshot(ckey);
        assert!(cvalue.is_some());

        let chdr = cvalue.unwrap();
        assert_eq!(chdr.len(), 0);

        // Timing samples.
        let tkey = "tkey".to_owned();
        histogram.register(tkey.clone());

        let t0 = Instant::now();
        let t1 = t0 + Duration::from_nanos(1692);
        let tsample = Sample::Timing(tkey.clone(), t0, t1, 73);
        histogram.update(&tsample);

        let tvalue = histogram.snapshot(tkey);
        assert!(tvalue.is_some());

        let thdr = tvalue.unwrap();
        assert_eq!(thdr.len(), 1);
        assert_eq!(thdr.max(), 1692);

        // Value samples.
        let vkey = "vkey".to_owned();
        histogram.register(vkey.clone());

        let vsample = Sample::Value(vkey.clone(), 22);
        histogram.update(&vsample);

        let vvalue = histogram.snapshot(vkey);
        assert!(vvalue.is_some());

        let vhdr = vvalue.unwrap();
        assert_eq!(vhdr.len(), 1);
        assert_eq!(vhdr.max(), 22);
    }

    #[test]
    fn test_windowed_histogram_rollover() {
        let mut wh = WindowedHistogram::new(Duration::new(5, 0), Duration::new(1, 0));
        let now = Instant::now();

        let merged = wh.merged();
        assert_eq!(merged.len(), 0);

        wh.update(1);
        wh.update(2);
        let merged = wh.merged();
        assert_eq!(merged.len(), 2);

        // Roll forward 3 seconds, should still have everything.
        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let merged = wh.merged();
        assert_eq!(merged.len(), 2);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let merged = wh.merged();
        assert_eq!(merged.len(), 2);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let merged = wh.merged();
        assert_eq!(merged.len(), 2);

        // Pump in some new values.
        wh.update(3);
        wh.update(4);
        wh.update(5);

        let merged = wh.merged();
        assert_eq!(merged.len(), 5);

        // Roll forward 3 seconds, and make sure the first two values are gone.
        // You might think this should be 2 seconds, but we have one extra bucket
        // allocated so that there's always a clear bucket that we can write into.
        // This means we have more than our total window, but only having the exact
        // number of buckets would mean we were constantly missing a bucket's worth
        // of granularity.
        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let merged = wh.merged();
        assert_eq!(merged.len(), 5);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let merged = wh.merged();
        assert_eq!(merged.len(), 5);

        let now = now + Duration::new(1, 0);
        wh.upkeep(now);
        let merged = wh.merged();
        assert_eq!(merged.len(), 3);
    }
}
