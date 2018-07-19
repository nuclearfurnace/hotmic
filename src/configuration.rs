use receiver::Receiver;
use std::hash::Hash;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::time::Duration;

/// A configuration builder for `Receiver`.
#[derive(Clone)]
pub struct Configuration<T> {
    metric_type: PhantomData<T>,
    pub(crate) capacity: usize,
    pub(crate) batch_size: usize,
    pub(crate) poll_delay: Option<Duration>,
}

impl<T> Default for Configuration<T> {
    fn default() -> Configuration<T> {
        Configuration {
            metric_type: PhantomData::<T>,
            capacity: 128,
            batch_size: 128,
            poll_delay: Some(Duration::from_millis(100)),
        }
    }
}

impl<T: Send + Eq + Hash + Display + Debug + Clone> Configuration<T> {
    /// Creates a new `Configuration` with default values.
    pub fn new() -> Configuration<T> {
        Default::default()
    }

    /// Sets the buffer capacity.
    ///
    /// Defaults to `128`.
    ///
    /// This controls how many buffers are pre-allocated, and conversely, how many buffers are able
    /// to be sent into the data channel without blocking.  If all buffers are consumed, any send
    /// activity by a `Sink` will block until a buffer is returned and made available.
    ///
    /// Tweaking this value allows for controlling the trade-off between memory consumption -- as
    /// all buffers are preallocated -- and throughput burst capabilities as a burst of metrics
    /// could quickly empty the buffer list and cause blocking.
    ///
    /// Generally speaking, sending and processing metrics is fast enough that the default value of
    /// 128 supports hundreds of thousands of samples per second.
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// Sets the batch size.
    ///
    /// Defaults to `128`.
    ///
    /// All `Sink`s will internally buffer up to `batch_size` samples before sending them to the
    /// `Sink`.
    ///
    /// The default batch size of 128 was chosen empirically to support a near-theoretical
    /// throughput limit given the work the `Sink` will do every every sample processed.  It will
    /// work very well if you have a high enough rate of metrics to continually flush the buffers,
    /// allowing you extremely low overhead and responsiveness.
    ///
    /// If your metric rate per `Sink` is low, or infrequent, or you want all metrics to have
    /// real-time values shown in snapshots, you should change this to a lower number.
    ///
    /// Batch sizes of 1 to 8 will provide roughly 425k samples/second of throughput per item, so
    /// 425k/samples sec for batch=1, 850k samples/sec for batch=2, and so on.
    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Sets the poll delay.
    ///
    /// Defaults to `100ms`.
    ///
    /// This controls the timeout that is used internal for polling the various data and control
    /// channels.  This should normally not be changed from default, as we depend on a regular
    /// cadence of cooperative yielding to be able to perform upkeep on histograms.
    pub fn poll_delay(mut self, poll_delay: Option<Duration>) -> Self {
        self.poll_delay = poll_delay;
        self
    }

    /// Create a `Receiver` based on this configuration.
    pub fn build(self) -> Receiver<T> {
        Receiver::from_config(self)
    }
}
