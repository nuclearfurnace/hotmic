use std::hash::Hash;
use fnv::FnvHashMap;
use hdrhistogram::Histogram as HdrHistogram;
use std::time::{Instant, Duration};
use super::Sample;

const TEN_MINS_NS: u64 = 10 * 60 * 1_000_000_000;

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
            let histogram = HdrHistogram::new_with_bounds(1, TEN_MINS_NS, 3).unwrap();
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
            self.buckets[self.bucket_index].clear();
            self.bucket_index += 1;
            self.bucket_index %= self.num_buckets;
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

fn duration_as_nanos(d: Duration) -> u64 {
    (d.as_secs() * 1_000_000_000) + d.subsec_nanos() as u64
}
