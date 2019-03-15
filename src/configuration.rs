use crate::{data::Percentile, receiver::Receiver};
use std::{fmt::Display, hash::Hash, marker::PhantomData, time::Duration};

/// A configuration builder for [`Receiver`].
#[derive(Clone)]
pub struct Configuration<T> {
    metric_type: PhantomData<T>,
    pub(crate) capacity: usize,
    pub(crate) batch_size: usize,
    pub(crate) histogram_window: Duration,
    pub(crate) histogram_granularity: Duration,
    pub(crate) percentiles: Vec<Percentile>,
}

impl<T> Default for Configuration<T> {
    fn default() -> Configuration<T> {
        Configuration {
            metric_type: PhantomData::<T>,
            capacity: 512,
            batch_size: 64,
            histogram_window: Duration::from_secs(10),
            histogram_granularity: Duration::from_secs(1),
            percentiles: default_percentiles(),
        }
    }
}

impl<T: Send + Eq + Hash + Display + Clone> Configuration<T> {
    /// Creates a new [`Configuration`] with default values.
    pub fn new() -> Configuration<T> {
        Default::default()
    }

    /// Sets the buffer capacity.
    ///
    /// Defaults to 512.
    ///
    /// This controls the size of the channel used to send metrics.  This channel is shared amongst
    /// all active sinks.  If this channel is full when sending a metric, that send will be blocked
    /// until the channel has free space.
    ///
    /// Tweaking this value allows for a trade-off between low memory consumption and throughput
    /// burst capabilities.  By default, we expect samples to occupy approximately 64 bytes.  Thus,
    /// at our default value, we preallocate roughly ~32KB.
    ///
    /// Generally speaking, sending and processing metrics is fast enough that the default value of
    /// 4096 supports millions of samples per second.
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// Sets the batch size.
    ///
    /// Defaults to 64.
    ///
    /// This controls the size of message batches that we collect for processing.  The only real
    /// reason to tweak this is to control the latency from the sender side.  Larger batches lower
    /// the ingest latency in the face of high metric ingest pressure at the cost of higher tail
    /// latencies.
    ///
    /// Long story short, you shouldn't need to change this, but it's here if you really do.
    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Sets the histogram configuration.
    ///
    /// Defaults to a 10 second window with 1 second granularity.
    ///
    /// This controls how long of a time frame the histogram will track, on a rolling window.
    /// We'll create enough underlying histogram buckets so that we have (window / granularity)
    /// buckets, and every interval that passes (granularity), we'll add a new bucket and drop the
    /// oldest one, thereby providing a rolling window.
    ///
    /// Histograms, under the hood, are hard-coded to track three significant digits, and will take
    /// a theoretical maximum of around 60KB per bucket, so a single histogram metric with the
    /// default window/granularity will take a maximum of around 600KB.
    ///
    /// In practice, this should be much smaller based on the maximum values pushed into the
    /// histogram, as the underlying histogram storage is automatically resized on the fly.
    pub fn histogram(mut self, window: Duration, granularity: Duration) -> Self {
        self.histogram_window = window;
        self.histogram_granularity = granularity;
        self
    }

    /// Sets the default percentiles for histograms.
    ///
    /// Defaults to min/p50/p95/p99/p999/max.
    ///
    /// This controls the percentiles we extract from histograms when taking a snapshot.
    /// Percentiles are represented in metrics as pXXX, where XXX is the percentile i.e. p99 is
    /// 99.0, p999 is 99.9, etc.  min and max are 0.0 and 100.0, respectively.
    pub fn percentiles(mut self, percentiles: &[f64]) -> Self {
        self.percentiles = percentiles.iter().cloned().map(Percentile::from).collect();
        self
    }

    /// Create a [`Receiver`] based on this configuration.
    pub fn build(self) -> Receiver<T> {
        Receiver::from_config(self)
    }
}

/// A default set of percentiles that should support most use cases.
fn default_percentiles() -> Vec<Percentile> {
    let mut p = Vec::new();
    p.push(Percentile::from(0.0));
    p.push(Percentile::from(50.0));
    p.push(Percentile::from(95.0));
    p.push(Percentile::from(99.0));
    p.push(Percentile::from(99.9));
    p.push(Percentile::from(100.0));
    p
}
