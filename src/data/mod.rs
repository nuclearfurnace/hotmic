use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use hdrhistogram::Histogram as HdrHistogram;
use std::{
    fmt::{self, Display},
    hash::Hash,
    marker::PhantomData,
};

pub mod counter;
pub mod gauge;
pub mod histogram;

pub(crate) use self::{counter::Counter, gauge::Gauge, histogram::Histogram};

/// Type of computation against aggregated/processed samples.
///
/// Facets are, in essence, views over given metrics: a count tallys up all the counts for a given
/// metric to keep a single, globally-consistent value for all the matching samples seen, etc.
///
/// More simply, facets are essentially the same thing as a metric type: counters, gauges,
/// histograms, etc.  We treat them a little different because callers are never directly saying
/// that they want to change the value of a counter, or histogram, they're saying that for a given
/// metric type, they care about certain facets.
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
    ///
    /// The histograms that back percentiles are currently hard-coded to track a windowed view of
    /// 60 seconds, with a 1 second interval.  That is, they only store the last 10 seconds worth
    /// of data that they've been given.
    TimingPercentile(T),

    /// Value-specific percentiles.
    ///
    /// The histograms that back percentiles are currently hard-coded to track a windowed view of
    /// 60 seconds, with a 1 second interval.  That is, they only store the last 10 seconds worth
    /// of data that they've been given.
    ValuePercentile(T),
}

/// A measurement.
///
/// Samples are the decoupled way of submitting data into the sink.  Likewise with facets, metric
/// types/measurements are slightly decoupled, into facets and samples, so that we can allow
/// slightly more complicated and unique combinations of facets.
///
/// If you wanted to track, all time, the count of a given metric, but also the distribution of the
/// metric, you would need both a counter and histogram, as histograms are windowed.  Instead of
/// creating two metric names, and having two separate calls, you can simply register both the
/// count and timing percentile facets, and make one call, and both things are tracked for you.
///
/// There are multiple sample types to support the different types of measurements, which each have
/// their own specific data they must carry.
#[derive(Debug)]
pub(crate) enum Sample<T> {
    /// A timed sample.
    ///
    /// Includes the start and end times, as well as a count field.
    ///
    /// The count field can represent amounts integral to the event,
    /// such as the number of bytes processed in the given time delta.
    Timing(T, u64, u64, u64),

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

/// An integer scoped metric key.
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub(crate) struct ScopedKey<T: Clone + Eq + Hash + Display>(usize, T);

impl<T: Clone + Eq + Hash + Display> ScopedKey<T> {
    pub(crate) fn id(&self) -> usize { self.0 }

    pub(crate) fn into_string_scoped(self, scope: String) -> StringScopedKey<T> { StringScopedKey(scope, self.1) }
}

/// A string scoped metric key.
#[derive(Clone, Hash, PartialEq, Eq, Debug)]
pub(crate) struct StringScopedKey<T: Clone + Eq + Hash + Display>(String, T);

impl<T: Clone + Hash + Eq + Display> Display for StringScopedKey<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, "{}", self.1)
        } else {
            write!(f, "{}.{}", self.0, self.1)
        }
    }
}

impl<T: Clone + Eq + Hash + Display> Facet<T> {
    pub(crate) fn into_scoped(self, scope_id: usize) -> Facet<ScopedKey<T>> {
        match self {
            Facet::Count(key) => Facet::Count(ScopedKey(scope_id, key)),
            Facet::Gauge(key) => Facet::Gauge(ScopedKey(scope_id, key)),
            Facet::TimingPercentile(key) => Facet::TimingPercentile(ScopedKey(scope_id, key)),
            Facet::ValuePercentile(key) => Facet::ValuePercentile(ScopedKey(scope_id, key)),
        }
    }
}

impl<T: Clone + Eq + Hash + Display> Sample<T> {
    pub(crate) fn into_scoped(self, scope_id: usize) -> Sample<ScopedKey<T>> {
        match self {
            Sample::Timing(key, start, end, count) => Sample::Timing(ScopedKey(scope_id, key), start, end, count),
            Sample::Count(key, value) => Sample::Count(ScopedKey(scope_id, key), value),
            Sample::Value(key, value) => Sample::Value(ScopedKey(scope_id, key), value),
        }
    }
}

/// A labeled percentile.
///
/// This represents a floating-point value from 0 to 100, with a string label to be used for
/// displaying the given percentile.
#[derive(Clone)]
pub struct Percentile(pub String, pub f64);

/// A default set of percentiles that should support most use cases.
///
/// Contains min (or 0.0), p50 (50.0), p90 (090.0), p99 (99.0), p999 (99.9) and max (100.0).
pub fn default_percentiles() -> Vec<Percentile> {
    let mut p = Vec::new();
    p.push(Percentile("min".to_owned(), 0.0));
    p.push(Percentile("p50".to_owned(), 50.0));
    p.push(Percentile("p90".to_owned(), 90.0));
    p.push(Percentile("p99".to_owned(), 99.0));
    p.push(Percentile("p999".to_owned(), 99.9));
    p.push(Percentile("max".to_owned(), 100.0));
    p
}

/// A point-in-time view of metric data.
#[derive(Default)]
pub struct Snapshot {
    signed_data: HashMap<String, i64, FnvBuildHasher>,
    unsigned_data: HashMap<String, u64, FnvBuildHasher>,
}

impl Snapshot {
    /// Gets the counter value for the given metric key.
    ///
    /// Returns `None` if the metric key has no counter value in this snapshot.
    pub fn count(&self, key: &str) -> Option<&i64> {
        let fkey = format!("{}_count", key);
        self.signed_data.get(&fkey)
    }

    /// Gets the gauge value for the given metric key.
    ///
    /// Returns `None` if the metric key has no gauge value in this snapshot.
    pub fn value(&self, key: &str) -> Option<&u64> {
        let fkey = format!("{}_value", key);
        self.unsigned_data.get(&fkey)
    }

    /// Gets the given timing percentile for given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub fn timing_percentile(&self, key: &str, percentile: Percentile) -> Option<&u64> {
        let fkey = format!("{}_ns_{}", key, percentile.0);
        self.unsigned_data.get(&fkey)
    }

    /// Gets the given value percentile for the given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub fn value_percentile(&self, key: &str, percentile: Percentile) -> Option<&u64> {
        let fkey = format!("{}_value_{}", key, percentile.0);
        self.unsigned_data.get(&fkey)
    }

    /// Gets a collection of the metrics with signed values.
    pub fn get_signed_data(&self) -> Vec<(String, i64)> {
        self.signed_data.iter().map(|(a, b)| (a.clone(), *b)).collect()
    }

    /// Gets a collection of the metrics with unsigned values.
    pub fn get_unsigned_data(&self) -> Vec<(String, u64)> {
        self.unsigned_data.iter().map(|(a, b)| (a.clone(), *b)).collect()
    }
}

/// Builder for creating a snapshot.
pub(crate) struct SnapshotBuilder<T> {
    inner: Snapshot,
    _marker: PhantomData<T>,
}

#[allow(dead_code)]
impl<T: Send + Eq + Hash + Send + Display + Clone> SnapshotBuilder<T> {
    /// Creates an empty `SnapshotBuilder`.
    pub(crate) fn new() -> SnapshotBuilder<T> {
        SnapshotBuilder {
            inner: Default::default(),
            _marker: PhantomData,
        }
    }

    /// Stores a counter value for the given metric key.
    pub(crate) fn set_count(&mut self, key: T, value: i64) {
        let fkey = format!("{}_count", key);
        self.inner.signed_data.insert(fkey, value);
    }

    /// Stores a gauge value for the given metric key.
    pub(crate) fn set_value(&mut self, key: T, value: u64) {
        let fkey = format!("{}_value", key);
        self.inner.unsigned_data.insert(fkey, value);
    }

    /// Sets timing percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_timing_percentiles(&mut self, key: T, h: HdrHistogram<u64>, percentiles: &[Percentile]) {
        for percentile in percentiles {
            let fkey = format!("{}_ns_{}", key, percentile.0);
            let value = h.value_at_percentile(percentile.1);
            self.inner.unsigned_data.insert(fkey, value);
        }
    }

    /// Sets value percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_value_percentiles(&mut self, key: T, h: HdrHistogram<u64>, percentiles: &[Percentile]) {
        for percentile in percentiles {
            let fkey = format!("{}_value_{}", key, percentile.0);
            let value = h.value_at_percentile(percentile.1);
            self.inner.unsigned_data.insert(fkey, value);
        }
    }

    /// Gets the counter value for the given metric key.
    ///
    /// Returns `None` if the metric key has no counter value in this snapshot.
    pub(crate) fn count(&self, key: &T) -> Option<&i64> {
        let fkey = format!("{}_count", key);
        self.inner.signed_data.get(&fkey)
    }

    /// Gets the gauge value for the given metric key.
    ///
    /// Returns `None` if the metric key has no gauge value in this snapshot.
    pub(crate) fn value(&self, key: &T) -> Option<&u64> {
        let fkey = format!("{}_value", key);
        self.inner.unsigned_data.get(&fkey)
    }

    /// Gets the given timing percentile for given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub(crate) fn timing_percentile(&self, key: &T, percentile: Percentile) -> Option<&u64> {
        let fkey = format!("{}_ns_{}", key, percentile.0);
        self.inner.unsigned_data.get(&fkey)
    }

    /// Gets the given value percentile for the given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub(crate) fn value_percentile(&self, key: &T, percentile: Percentile) -> Option<&u64> {
        let fkey = format!("{}_value_{}", key, percentile.0);
        self.inner.unsigned_data.get(&fkey)
    }

    /// Converts this `SnapshotBuilder` into a `Snapshot`.
    pub(crate) fn into_inner(self) -> Snapshot { self.inner }
}

#[cfg(test)]
mod tests {
    use super::{Percentile, SnapshotBuilder};
    use hdrhistogram::Histogram;

    #[test]
    fn test_snapshot_simple_set_and_get() {
        let key = "ok".to_owned();
        let mut snapshot = SnapshotBuilder::new();
        snapshot.set_count(key.clone(), 1);
        snapshot.set_value(key.clone(), 42);

        assert_eq!(snapshot.count(&key).unwrap(), &1);
        assert_eq!(snapshot.value(&key).unwrap(), &42);
    }

    #[test]
    fn test_snapshot_percentiles() {
        let mut snapshot = SnapshotBuilder::new();

        {
            let mut h1 = Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap();
            h1.saturating_record(500_000);
            h1.saturating_record(750_000);
            h1.saturating_record(1_000_000);
            h1.saturating_record(1_250_000);

            let tkey = "ok".to_owned();
            let mut tpercentiles = Vec::new();
            tpercentiles.push(Percentile("min".to_owned(), 0.0));
            tpercentiles.push(Percentile("p50".to_owned(), 50.0));
            tpercentiles.push(Percentile("p99".to_owned(), 99.0));
            tpercentiles.push(Percentile("max".to_owned(), 100.0));

            snapshot.set_timing_percentiles(tkey.clone(), h1, &tpercentiles);

            let min_tpercentile = snapshot.timing_percentile(&tkey, tpercentiles[0].clone());
            let p50_tpercentile = snapshot.timing_percentile(&tkey, tpercentiles[1].clone());
            let p99_tpercentile = snapshot.timing_percentile(&tkey, tpercentiles[2].clone());
            let max_tpercentile = snapshot.timing_percentile(&tkey, tpercentiles[3].clone());
            let fake_tpercentile = snapshot.timing_percentile(&tkey, Percentile("fake".to_owned(), 63.0));

            assert!(min_tpercentile.is_some());
            assert!(p50_tpercentile.is_some());
            assert!(p99_tpercentile.is_some());
            assert!(max_tpercentile.is_some());
            assert!(fake_tpercentile.is_none());
        }

        {
            let mut h2 = Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap();
            h2.saturating_record(500_000);
            h2.saturating_record(750_000);
            h2.saturating_record(1_000_000);
            h2.saturating_record(1_250_000);

            let vkey = "ok".to_owned();
            let mut vpercentiles = Vec::new();
            vpercentiles.push(Percentile("min".to_owned(), 0.0));
            vpercentiles.push(Percentile("p50".to_owned(), 50.0));
            vpercentiles.push(Percentile("p99".to_owned(), 99.0));
            vpercentiles.push(Percentile("max".to_owned(), 100.0));

            snapshot.set_value_percentiles(vkey.clone(), h2, &vpercentiles);

            let min_vpercentile = snapshot.value_percentile(&vkey, vpercentiles[0].clone());
            let p50_vpercentile = snapshot.value_percentile(&vkey, vpercentiles[1].clone());
            let p99_vpercentile = snapshot.value_percentile(&vkey, vpercentiles[2].clone());
            let max_vpercentile = snapshot.value_percentile(&vkey, vpercentiles[3].clone());
            let fake_vpercentile = snapshot.value_percentile(&vkey, Percentile("fake".to_owned(), 63.0));

            assert!(min_vpercentile.is_some());
            assert!(p50_vpercentile.is_some());
            assert!(p99_vpercentile.is_some());
            assert!(max_vpercentile.is_some());
            assert!(fake_vpercentile.is_none());
        }
    }
}
