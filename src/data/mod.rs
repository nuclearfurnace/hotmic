use std::{
    fmt::{self, Display},
    hash::Hash,
};

pub mod counter;
pub mod gauge;
pub mod histogram;
pub mod snapshot;

pub(crate) use self::{counter::Counter, gauge::Gauge, histogram::Histogram, snapshot::Snapshot};

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
pub(crate) struct ScopedKey<T: Clone + Eq + Hash + Display>(u64, T);

impl<T: Clone + Eq + Hash + Display> ScopedKey<T> {
    pub(crate) fn id(&self) -> u64 { self.0 }

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
    pub(crate) fn into_scoped(self, scope_id: u64) -> Sample<ScopedKey<T>> {
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
#[derive(Derivative, Debug, Clone)]
#[derivative(Hash, PartialEq)]
pub struct Percentile {
    label: String,

    #[derivative(Hash = "ignore")]
    #[derivative(PartialEq = "ignore")]
    value: f64,
}

impl Percentile {
    /// Gets the standardized label for this percentile value.
    ///
    /// This follows the convention of `pXXX`, where `xxx` represents the percentage.  For example,
    /// for the 99th percentile, you would get `p99`.  For the 99.9th percentile, you would get `p999`.
    pub fn label(&self) -> &str { self.label.as_str() }

    /// Gets the actual percentile value.
    pub fn percentile(&self) -> f64 { self.value }

    /// Gets the percentile value as a quantile.
    pub fn as_quantile(&self) -> f64 { self.value / 100.0 }
}

impl Eq for Percentile {}

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

        Percentile { label, value: clamped }
    }
}
