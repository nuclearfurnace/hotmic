use super::receiver::Receiver;
use std::{fmt::Display, hash::Hash, marker::PhantomData};

/// A configuration builder for `Receiver`.
#[derive(Clone)]
pub struct Configuration<T> {
    metric_type: PhantomData<T>,
    pub(crate) capacity: usize,
}

impl<T> Default for Configuration<T> {
    fn default() -> Configuration<T> {
        Configuration {
            metric_type: PhantomData::<T>,
            capacity: 4096,
        }
    }
}

impl<T: Send + Eq + Hash + Display + Clone> Configuration<T> {
    /// Creates a new `Configuration` with default values.
    pub fn new() -> Configuration<T> { Default::default() }

    /// Sets the buffer capacity.
    ///
    /// Defaults to `4096`.
    ///
    /// This controls the size of the channel used to send metrics.  This channel is shared amongst
    /// all active sinks.  If this channel is full when sending a metric, that send will be blocked
    /// until the channel has free space.
    ///
    /// Tweaking this value allows for a trade-off between low memory consumption and throughput
    /// burst capabilities.  By default, we expect samples to occupy approximately 64 bytes.  Thus,
    /// at our default value, we preallocate roughly ~232kB.
    ///
    /// Generally speaking, sending and processing metrics is fast enough that the default value of
    /// 4096 supports millions of samples per second.
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// Create a `Receiver` based on this configuration.
    pub fn build(self) -> Receiver<T> { Receiver::from_config(self) }
}
