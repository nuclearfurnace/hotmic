use fnv::FnvBuildHasher;
use hashbrown::HashMap;
use hdrhistogram::Histogram as HdrHistogram;
use serde::ser::{Serialize, SerializeMap, Serializer};
use std::{
    fmt::{self, Display},
    hash::Hash,
    marker::PhantomData,
};

pub mod counter;
pub mod gauge;
pub mod histogram;

pub(crate) use self::{counter::Counter, gauge::Gauge, histogram::Histogram};

/// A measurement.
///
/// Samples are the decoupled way of submitting data into the sink.
#[derive(Debug)]
pub(crate) enum Sample<T> {
    /// A counter delta.
    ///
    /// The value is added directly to the existing counter, and so negative deltas will decrease
    /// the counter, and positive deltas will increase the counter.
    Count(T, i64),

    /// A single value, also known as a gauge.
    ///
    /// Values operate in last-write-wins mode.
    ///
    /// Values themselves cannot be incremented or decremented, so you must hold them externally
    /// before sending them.
    Gauge(T, u64),

    /// A timed sample.
    ///
    /// Includes the start and end times, as well as a count field.
    ///
    /// The count field can represent amounts integral to the event, such as the number of bytes
    /// processed in the given time delta.
    TimingHistogram(T, u64, u64, u64),

    /// A single value measured over time.
    ///
    /// Unlike a gauge, where the value is only ever measured at a point in time, value histogram
    /// measure values over time, and their distribution.  This is nearly identical to timing
    /// histograms, since the end result is just a single number, but we don't spice it up with
    /// special unit labels or anything.
    ValueHistogram(T, u64),
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

impl<T: Clone + Eq + Hash + Display> Sample<T> {
    pub(crate) fn into_scoped(self, scope_id: usize) -> Sample<ScopedKey<T>> {
        match self {
            Sample::Count(key, value) => Sample::Count(ScopedKey(scope_id, key), value),
            Sample::Gauge(key, value) => Sample::Gauge(ScopedKey(scope_id, key), value),
            Sample::TimingHistogram(key, start, end, count) => {
                Sample::TimingHistogram(ScopedKey(scope_id, key), start, end, count)
            },
            Sample::ValueHistogram(key, count) => Sample::ValueHistogram(ScopedKey(scope_id, key), count),
        }
    }
}

/// A labeled percentile.
///
/// This represents a floating-point value from 0 to 100, with a string label to be used for
/// displaying the given percentile.
#[derive(Clone)]
pub(crate) struct Percentile(pub String, pub f64);

impl From<f64> for Percentile {
    fn from(p: f64) -> Self {
        // Force our value between +0.0 and +100.0.
        let clamped = p.max(0.0);
        let clamped = clamped.min(100.0);

        let raw_label = format!("{}", clamped);
        let label = match raw_label.as_str() {
            "0" => "min".to_string(),
            "100" => "max".to_string(),
            _ => {
                let raw = format!("p{}", clamped);
                raw.replace(".", "")
            },
        };

        Percentile(label, clamped)
    }
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
    pub fn count(&self, key: &str) -> Option<&i64> { self.signed_data.get(key) }

    /// Gets the gauge value for the given metric key.
    ///
    /// Returns `None` if the metric key has no gauge value in this snapshot.
    pub fn gauge(&self, key: &str) -> Option<&u64> { self.unsigned_data.get(key) }

    /// Gets the given timing percentile for given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub fn timing_histogram(&self, key: &str, percentile: f64) -> Option<&u64> {
        let p = Percentile::from(percentile);
        let fkey = format!("{}_ns_{}", key, p.0);
        self.unsigned_data.get(&fkey)
    }

    /// Gets the given value percentile for the given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub fn value_histogram(&self, key: &str, percentile: f64) -> Option<&u64> {
        let p = Percentile::from(percentile);
        let fkey = format!("{}_{}", key, p.0);
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

impl Serialize for Snapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let field_count = self.signed_data.len() + self.unsigned_data.len();
        let mut map = serializer.serialize_map(Some(field_count))?;
        for (k, v) in &self.signed_data {
            map.serialize_entry(k, v)?;
        }
        for (k, v) in &self.unsigned_data {
            map.serialize_entry(k, v)?;
        }
        map.end()
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
        let fkey = format!("{}", key);
        self.inner.signed_data.insert(fkey, value);
    }

    /// Stores a gauge value for the given metric key.
    pub(crate) fn set_gauge(&mut self, key: T, value: u64) {
        let fkey = format!("{}", key);
        self.inner.unsigned_data.insert(fkey, value);
    }

    /// Sets timing percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_timing_histogram(&mut self, key: T, h: HdrHistogram<u64>, percentiles: &[Percentile]) {
        for percentile in percentiles {
            let fkey = format!("{}_ns_{}", key, percentile.0);
            let value = h.value_at_percentile(percentile.1);
            self.inner.unsigned_data.insert(fkey, value);
        }
    }

    /// Sets value percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_value_histogram(&mut self, key: T, h: HdrHistogram<u64>, percentiles: &[Percentile]) {
        for percentile in percentiles {
            let fkey = format!("{}_{}", key, percentile.0);
            let value = h.value_at_percentile(percentile.1);
            self.inner.unsigned_data.insert(fkey, value);
        }
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
        let mut sb = SnapshotBuilder::new();
        sb.set_count(key.clone(), 1);
        sb.set_gauge(key.clone(), 42);

        let snapshot = sb.into_inner();
        assert_eq!(snapshot.count(&key).unwrap(), &1);
        assert_eq!(snapshot.gauge(&key).unwrap(), &42);
    }

    #[test]
    fn test_snapshot_percentiles() {
        {
            let mut sb = SnapshotBuilder::new();
            let mut h1 = Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap();
            h1.saturating_record(500_000);
            h1.saturating_record(750_000);
            h1.saturating_record(1_000_000);
            h1.saturating_record(1_250_000);

            let tkey = "ok".to_owned();
            let mut tpercentiles = Vec::new();
            tpercentiles.push(Percentile::from(0.0));
            tpercentiles.push(Percentile::from(50.0));
            tpercentiles.push(Percentile::from(99.0));
            tpercentiles.push(Percentile::from(100.0));

            sb.set_timing_histogram(tkey.clone(), h1, &tpercentiles);

            let snapshot = sb.into_inner();
            let min_tpercentile = snapshot.timing_histogram(&tkey, tpercentiles[0].1);
            let p50_tpercentile = snapshot.timing_histogram(&tkey, tpercentiles[1].1);
            let p99_tpercentile = snapshot.timing_histogram(&tkey, tpercentiles[2].1);
            let max_tpercentile = snapshot.timing_histogram(&tkey, tpercentiles[3].1);
            let fake_tpercentile = snapshot.timing_histogram(&tkey, 63.0);

            assert!(min_tpercentile.is_some());
            assert!(p50_tpercentile.is_some());
            assert!(p99_tpercentile.is_some());
            assert!(max_tpercentile.is_some());
            assert!(fake_tpercentile.is_none());
        }

        {
            let mut sb = SnapshotBuilder::new();
            let mut h2 = Histogram::<u64>::new_with_bounds(1, u64::max_value(), 3).unwrap();
            h2.saturating_record(500_000);
            h2.saturating_record(750_000);
            h2.saturating_record(1_000_000);
            h2.saturating_record(1_250_000);

            let vkey = "ok".to_owned();
            let mut vpercentiles = Vec::new();
            vpercentiles.push(Percentile::from(0.0));
            vpercentiles.push(Percentile::from(50.0));
            vpercentiles.push(Percentile::from(99.0));
            vpercentiles.push(Percentile::from(100.0));

            sb.set_value_histogram(vkey.clone(), h2, &vpercentiles);

            let snapshot = sb.into_inner();
            let min_vpercentile = snapshot.value_histogram(&vkey, vpercentiles[0].1);
            let p50_vpercentile = snapshot.value_histogram(&vkey, vpercentiles[1].1);
            let p99_vpercentile = snapshot.value_histogram(&vkey, vpercentiles[2].1);
            let max_vpercentile = snapshot.value_histogram(&vkey, vpercentiles[3].1);
            let fake_vpercentile = snapshot.value_histogram(&vkey, 63.0);

            assert!(min_vpercentile.is_some());
            assert!(p50_vpercentile.is_some());
            assert!(p99_vpercentile.is_some());
            assert!(max_vpercentile.is_some());
            assert!(fake_vpercentile.is_none());
        }
    }

    #[test]
    fn test_percentiles() {
        let min_p = Percentile::from(0.0);
        assert_eq!(min_p.0, "min");

        let max_p = Percentile::from(100.0);
        assert_eq!(max_p.0, "max");

        let clamped_min_p = Percentile::from(-20.0);
        assert_eq!(clamped_min_p.0, "min");
        assert_eq!(clamped_min_p.1, 0.0);

        let clamped_max_p = Percentile::from(1442.0);
        assert_eq!(clamped_max_p.0, "max");
        assert_eq!(clamped_max_p.1, 100.0);

        let p99_p = Percentile::from(99.0);
        assert_eq!(p99_p.0, "p99");

        let p999_p = Percentile::from(99.9);
        assert_eq!(p999_p.0, "p999");

        let p9999_p = Percentile::from(99.99);
        assert_eq!(p9999_p.0, "p9999");
    }
}
