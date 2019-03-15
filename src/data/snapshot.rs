use super::Percentile;
use hdrhistogram::Histogram as HdrHistogram;
use std::collections::HashMap;
use std::fmt::Display;

#[derive(Debug, PartialEq, Eq)]
pub enum TypedMeasurement {
    Counter(String, i64),
    Gauge(String, u64),
    TimingHistogram(String, SummarizedHistogram),
    ValueHistogram(String, SummarizedHistogram),
}

/// A point-in-time view of metric data.
#[derive(Default)]
pub struct Snapshot {
    measurements: Vec<TypedMeasurement>,
}

impl Snapshot {
    /// Stores a counter value for the given metric key.
    pub(crate) fn set_count<T>(&mut self, key: T, value: i64)
    where
        T: Display,
    {
        self.measurements
            .push(TypedMeasurement::Counter(key.to_string(), value));
    }

    /// Stores a gauge value for the given metric key.
    pub(crate) fn set_gauge<T>(&mut self, key: T, value: u64)
    where
        T: Display,
    {
        self.measurements.push(TypedMeasurement::Gauge(key.to_string(), value));
    }

    /// Sets timing percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_timing_histogram<T>(&mut self, key: T, h: HdrHistogram<u64>, percentiles: &[Percentile])
    where
        T: Display,
    {
        let summarized = SummarizedHistogram::from_histogram(h, percentiles);
        self.measurements
            .push(TypedMeasurement::TimingHistogram(key.to_string(), summarized));
    }

    /// Sets value percentiles for the given metric key.
    ///
    /// From the given `HdrHistogram`, all the specific `percentiles` will be extracted and stored.
    pub(crate) fn set_value_histogram<T>(&mut self, key: T, h: HdrHistogram<u64>, percentiles: &[Percentile])
    where
        T: Display,
    {
        let summarized = SummarizedHistogram::from_histogram(h, percentiles);
        self.measurements
            .push(TypedMeasurement::ValueHistogram(key.to_string(), summarized));
    }

    /// Converts this [`Snapshot`] into `[`SimpleSnapshot`].
    ///
    /// [`SimpleSnapshot`] provides a programmatic interface to more easily sift through the
    /// metrics within, without needing to evaluate all of them.
    pub fn into_simple(self) -> SimpleSnapshot {
        SimpleSnapshot::from_snapshot(self)
    }

    /// Converts this [`Snapshot`] to the underlying vector of measurements.
    pub fn into_vec(self) -> Vec<TypedMeasurement> {
        self.measurements
    }
}

/// A user-friendly metric snapshot that allows easy retrieval of values.
///
/// This is good for programmatic exploration of values, whereas [`Snapshot`] is designed around
/// being consumed by output adapters that send metrics to external collection systems.
#[derive(Default)]
pub struct SimpleSnapshot {
    pub(crate) counters: HashMap<String, i64>,
    pub(crate) gauges: HashMap<String, u64>,
    pub(crate) timings: HashMap<String, SummarizedHistogram>,
    pub(crate) values: HashMap<String, SummarizedHistogram>,
}

impl SimpleSnapshot {
    pub(crate) fn from_snapshot(s: Snapshot) -> Self {
        let mut ss = SimpleSnapshot::default();
        for metric in s.into_vec() {
            match metric {
                TypedMeasurement::Counter(key, value) => {
                    ss.counters.insert(key, value);
                }
                TypedMeasurement::Gauge(key, value) => {
                    ss.gauges.insert(key, value);
                }
                TypedMeasurement::TimingHistogram(key, value) => {
                    ss.timings.insert(key, value);
                }
                TypedMeasurement::ValueHistogram(key, value) => {
                    ss.values.insert(key, value);
                }
            }
        }
        ss
    }

    /// Gets the counter value for the given metric key.
    ///
    /// Returns `None` if the metric key has no counter value in this snapshot.
    pub fn count(&self, key: &str) -> Option<i64> {
        self.counters.get(key).cloned()
    }

    /// Gets the gauge value for the given metric key.
    ///
    /// Returns `None` if the metric key has no gauge value in this snapshot.
    pub fn gauge(&self, key: &str) -> Option<u64> {
        self.gauges.get(key).cloned()
    }

    /// Gets the given timing percentile for given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub fn timing_histogram(&self, key: &str, percentile: f64) -> Option<u64> {
        let p = Percentile::from(percentile);
        self.timings.get(key).and_then(|s| s.measurements().get(&p)).cloned()
    }

    /// Gets the given value percentile for the given metric key.
    ///
    /// Returns `None` if the metric key has no value at the given percentile in this snapshot.
    pub fn value_histogram(&self, key: &str, percentile: f64) -> Option<u64> {
        let p = Percentile::from(percentile);
        self.values.get(key).and_then(|s| s.measurements().get(&p)).cloned()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SummarizedHistogram {
    count: u64,
    measurements: HashMap<Percentile, u64>,
}

impl SummarizedHistogram {
    pub fn from_histogram(histogram: HdrHistogram<u64>, percentiles: &[Percentile]) -> Self {
        let mut measurements = HashMap::default();
        let count = histogram.len();

        for percentile in percentiles {
            let value = histogram.value_at_percentile(percentile.value);
            measurements.insert(percentile.clone(), value);
        }

        SummarizedHistogram { count, measurements }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn measurements(&self) -> &HashMap<Percentile, u64> {
        &self.measurements
    }
}

#[cfg(test)]
mod tests {
    use super::{Percentile, Snapshot, TypedMeasurement};
    use hdrhistogram::Histogram;

    #[test]
    fn test_snapshot_simple_set_and_get() {
        let key = "ok".to_owned();
        let mut snapshot = Snapshot::default();
        snapshot.set_count(key.clone(), 1);
        snapshot.set_gauge(key.clone(), 42);

        let values = snapshot.into_vec();

        assert_eq!(values[0], TypedMeasurement::Counter("ok".to_owned(), 1));
        assert_eq!(values[1], TypedMeasurement::Gauge("ok".to_owned(), 42));
    }

    #[test]
    fn test_snapshot_percentiles() {
        {
            let mut snapshot = Snapshot::default();
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
            let fake = Percentile::from(63.0);

            snapshot.set_timing_histogram(tkey.clone(), h1, &tpercentiles);

            let values = snapshot.into_vec();
            match values.get(0) {
                Some(TypedMeasurement::TimingHistogram(key, summary)) => {
                    assert_eq!(key, "ok");
                    assert_eq!(summary.count(), 4);

                    let min_tpercentile = summary.measurements().get(&tpercentiles[0]);
                    let p50_tpercentile = summary.measurements().get(&tpercentiles[1]);
                    let p99_tpercentile = summary.measurements().get(&tpercentiles[2]);
                    let max_tpercentile = summary.measurements().get(&tpercentiles[3]);
                    let fake_tpercentile = summary.measurements().get(&fake);

                    assert!(min_tpercentile.is_some());
                    assert!(p50_tpercentile.is_some());
                    assert!(p99_tpercentile.is_some());
                    assert!(max_tpercentile.is_some());
                    assert!(fake_tpercentile.is_none());
                }
                _ => panic!("expected timing histogram value! actual: {:?}", values[0]),
            }
        }

        {
            let mut snapshot = Snapshot::default();
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
            let fake = Percentile::from(63.0);

            snapshot.set_value_histogram(tkey.clone(), h1, &tpercentiles);

            let values = snapshot.into_vec();
            match values.get(0) {
                Some(TypedMeasurement::ValueHistogram(key, summary)) => {
                    assert_eq!(key, "ok");
                    assert_eq!(summary.count(), 4);

                    let min_tpercentile = summary.measurements().get(&tpercentiles[0]);
                    let p50_tpercentile = summary.measurements().get(&tpercentiles[1]);
                    let p99_tpercentile = summary.measurements().get(&tpercentiles[2]);
                    let max_tpercentile = summary.measurements().get(&tpercentiles[3]);
                    let fake_tpercentile = summary.measurements().get(&fake);

                    assert!(min_tpercentile.is_some());
                    assert!(p50_tpercentile.is_some());
                    assert!(p99_tpercentile.is_some());
                    assert!(max_tpercentile.is_some());
                    assert!(fake_tpercentile.is_none());
                }
                _ => panic!("expected value histogram value! actual: {:?}", values[0]),
            }
        }
    }

    #[test]
    fn test_percentiles() {
        let min_p = Percentile::from(0.0);
        assert_eq!(min_p.label(), "min");

        let max_p = Percentile::from(100.0);
        assert_eq!(max_p.label(), "max");

        let clamped_min_p = Percentile::from(-20.0);
        assert_eq!(clamped_min_p.label(), "min");
        assert_eq!(clamped_min_p.percentile(), 0.0);

        let clamped_max_p = Percentile::from(1442.0);
        assert_eq!(clamped_max_p.label(), "max");
        assert_eq!(clamped_max_p.percentile(), 100.0);

        let p99_p = Percentile::from(99.0);
        assert_eq!(p99_p.label(), "p99");

        let p999_p = Percentile::from(99.9);
        assert_eq!(p999_p.label(), "p999");

        let p9999_p = Percentile::from(99.99);
        assert_eq!(p9999_p.label(), "p9999");
    }
}
