use std::marker::PhantomData;
use std::time::Instant;
use fnv::FnvHashMap;
use std::hash::Hash;
use std::fmt::Display;
use hdrhistogram::Histogram as HdrHistogram;

pub mod counter;
pub mod gauge;
pub mod histogram;

pub use self::counter::Counter;
pub use self::gauge::Gauge;
pub use self::histogram::Histogram;

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub enum Facet<T> {
    /// A count.
    ///
    /// This could be the number of timing samples seen for a given metric,
    /// or the total count if modified via count samples.
    Count(T),

    /// A gauge.
    ///
    /// Gauges are singluar values, and operate in last-write-wins mode.
    Gauge(T),

    /// Timing-specific percentiles.
    TimingPercentile(T),

    /// Value-specific percentiles.
    ValuePercentile(T),
}

#[derive(Debug)]
pub enum Sample<T>
{
    /// A timed sample.
    ///
    /// Includes the start and end times, as well as a count field.
    ///
    /// The count field can represent amounts integral to the event,
    /// such as the number of bytes processed in the given time delta.
    Timing(T, Instant, Instant, u64),

    /// A counter delta.
    ///
    /// The value is added directly to the existing counter, and so
    /// negative deltas will decrease the counter, and positive deltas
    /// will increase the counter.
    Count(T, i64),

    /// A single value, also known as a gauge.
    ///
    /// Values operate in last-write-wins mode.
    ///
    /// Values themselves cannot be incremented or decremented, so you
    /// must hold them externally before sending them.
    Value(T, u64),
}

pub struct Quantile(pub String, pub f64);

pub fn default_quantiles() -> Vec<Quantile> {
    let mut p = Vec::new();
    p.push(Quantile("min".to_owned(), 0.0));
    p.push(Quantile("p50".to_owned(), 0.5));
    p.push(Quantile("p90".to_owned(), 0.9));
    p.push(Quantile("p99".to_owned(), 0.99));
    p.push(Quantile("p999".to_owned(), 0.999));
    p.push(Quantile("max".to_owned(), 1.0));
    p
}

pub struct Snapshot<T> {
    marker: PhantomData<T>,
    pub signed_data: FnvHashMap<String, i64>,
    pub unsigned_data: FnvHashMap<String, u64>,
}

impl<T: Send + Eq + Hash + Send + Display + Clone> Snapshot<T> {
    pub fn new() -> Snapshot<T> {
        Snapshot {
            marker: PhantomData,
            signed_data: FnvHashMap::default(),
            unsigned_data: FnvHashMap::default(),
        }
    }

    pub fn set_count(&mut self, key: T, value: i64) {
        let fkey = format!("{}_count", key);
        self.signed_data.insert(fkey, value);
    }

    pub fn set_value(&mut self, key: T, value: u64) {
        let fkey = format!("{}_value", key);
        self.unsigned_data.insert(fkey, value);
    }

    pub fn set_timing_quantile(&mut self, key: T, h: HdrHistogram<u64>, quantiles: &[Quantile]) {
        for quantile in quantiles {
            let fkey = format!("{}_ns_{}", key, quantile.0);
            let value = h.value_at_quantile(quantile.1);
            self.unsigned_data.insert(fkey, value);
        }
    }

    pub fn set_value_quantile(&mut self, key: T, h: HdrHistogram<u64>, quantiles: &[Quantile]) {
        for quantile in quantiles {
            let fkey = format!("{}_value_{}", key, quantile.0);
            let value = h.value_at_quantile(quantile.1);
            self.unsigned_data.insert(fkey, value);
        }
    }

    pub fn count(&self, key: &T) -> Option<&i64> {
        let fkey = format!("{}_count", key);
        self.signed_data.get(&fkey)
    }

    pub fn value(&self, key: &T) -> Option<&u64> {
        let fkey = format!("{}_value", key);
        self.unsigned_data.get(&fkey)
    }

    pub fn timing_quantile(&self, key: &T, quantile: Quantile) -> Option<&u64> {
        let fkey = format!("{}_ns_{}", key, quantile.0);
        self.unsigned_data.get(&fkey)
    }

    pub fn value_quantile(&self, key: &T, quantile: Quantile) -> Option<&u64> {
        let fkey = format!("{}_value_{}", key, quantile.0);
        self.unsigned_data.get(&fkey)
    }
}
