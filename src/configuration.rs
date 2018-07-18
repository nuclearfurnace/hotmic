use sink::Sink;
use std::hash::Hash;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;
use std::time::Duration;

#[derive(Clone)]
pub struct Configuration<T> {
    metric_type: PhantomData<T>,
    pub capacity: usize,
    pub batch_size: usize,
    pub poll_delay: Option<Duration>,
}

impl<T> Default for Configuration<T> {
    fn default() -> Configuration<T> {
        Configuration {
            metric_type: PhantomData::<T>,
            capacity: 128,
            batch_size: 128,
            poll_delay: None,
        }
    }
}

impl<T: Send + Eq + Hash + Display + Debug + Clone> Configuration<T> {
    pub fn new() -> Configuration<T> {
        Default::default()
    }

    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn poll_delay(mut self, delay: Option<Duration>) -> Self {
        self.poll_delay = delay;
        self
    }

    pub fn build(self) -> Sink<T> {
        Sink::from_config(self)
    }
}
