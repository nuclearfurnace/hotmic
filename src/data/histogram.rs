use crate::helper::duration_as_nanos;
use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use hdrhistogram::Histogram as HdrHistogram;
use std::{
    hash::Hash,
    time::{Duration, Instant},
};

pub(crate) struct Histogram<T> {
    window: Duration,
    granularity: Duration,
    data: HashMap<T, WindowedHistogram, FnvBuildHasher>,
}

impl<T: Clone + Eq + Hash> Histogram<T> {
    pub fn new(window: Duration, granularity: Duration) -> Histogram<T> {
        Histogram {
            window,
            granularity,
            data: HashMap::<T, WindowedHistogram, FnvBuildHasher>::default(),
        }
    }

    pub fn update(&mut self, key: T, value: u64) {
        if let Some(wh) = self.data.get_mut(&key) {
            wh.update(value);
        } else {
            let mut wh = WindowedHistogram::new(self.window, self.granularity);
            wh.update(value);
            let _ = self.data.insert(key, wh);
        }
    }

    pub fn upkeep(&mut self, at: Instant) {
        for (_, histogram) in self.data.iter_mut() {
            histogram.upkeep(at);
        }
    }

    pub fn values(&self) -> Vec<(T, HdrHistogram<u64>)> {
        self.data.iter().map(|(k, v)| (k.clone(), v.merged())).collect()
    }
}

pub(crate) struct WindowedHistogram {
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
            buckets,
            num_buckets,
            bucket_index: 0,
            last_upkeep: Instant::now(),
            granularity,
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

    pub fn update(&mut self, value: u64) { self.buckets[self.bucket_index].saturating_record(value); }

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
    use super::{Histogram, WindowedHistogram};
    use std::time::{Duration, Instant};

    #[test]
    fn test_histogram_simple_update() {
        let mut histogram = Histogram::new(Duration::new(5, 0), Duration::new(1, 0));

        let key = "foo";
        histogram.update(key, 1245);

        let values = histogram.values();
        assert_eq!(values.len(), 1);

        let hdr = &values[0].1;
        assert_eq!(hdr.len(), 1);
        assert_eq!(hdr.max(), 1245);
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
