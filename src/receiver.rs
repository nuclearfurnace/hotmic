use super::{
    configuration::Configuration,
    control::{ControlMessage, Controller},
    data::{
        default_percentiles, Counter, Facet, Gauge, Histogram, Percentile, Sample, ScopedKey, Snapshot, SnapshotBuilder,
    },
    sink::Sink,
};
use crossbeam_channel::{self, bounded, tick, TryRecvError};
use std::{
    collections::HashSet,
    fmt::Display,
    hash::Hash,
    time::{Duration, Instant},
};

/// Metrics receiver which aggregates and processes samples.
pub struct Receiver<T: Clone + Eq + Hash + Display + Send> {
    // Sample aggregation machinery.
    data_tx: crossbeam_channel::Sender<Sample<ScopedKey<T>>>,
    data_rx: crossbeam_channel::Receiver<Sample<ScopedKey<T>>>,
    control_tx: crossbeam_channel::Sender<ControlMessage<ScopedKey<T>>>,
    control_rx: crossbeam_channel::Receiver<ControlMessage<ScopedKey<T>>>,
    facets: HashSet<Facet<ScopedKey<T>>>,

    // Metric machinery.
    counter: Counter<ScopedKey<T>>,
    gauge: Gauge<ScopedKey<T>>,
    histogram: Histogram<ScopedKey<T>>,
    percentiles: Vec<Percentile>,
}

impl<T: Clone + Eq + Hash + Display + Send> Receiver<T> {
    pub(crate) fn from_config(conf: Configuration<T>) -> Receiver<T> {
        // Create our data, control, and buffer channels.
        let (data_tx, data_rx) = bounded(conf.capacity);
        let (control_tx, control_rx) = bounded(1024);

        Receiver {
            data_tx,
            data_rx,
            control_tx,
            control_rx,
            facets: HashSet::new(),
            counter: Counter::new(),
            gauge: Gauge::new(),
            histogram: Histogram::new(Duration::from_secs(10), Duration::from_secs(1)),
            percentiles: default_percentiles(),
        }
    }

    /// Gets a builder to configure a `Receiver` instance with.
    pub fn builder() -> Configuration<T> { Configuration::default() }

    /// Creates a `Sink` bound to this receiver.
    pub fn get_sink(&self) -> Sink<T> { Sink::new(self.data_tx.clone(), self.control_tx.clone()) }

    /// Creates a `Controller` bound to this receiver.
    pub fn get_controller(&self) -> Controller<T> { Controller::new(self.control_tx.clone()) }

    fn get_snapshot(&mut self) -> Snapshot {
        let mut snapshot = SnapshotBuilder::new();
        for facet in &self.facets {
            match *facet {
                Facet::Count(ref key) => snapshot.set_count(key.clone(), self.counter.value(key.clone())),
                Facet::Gauge(ref key) => snapshot.set_value(key.clone(), self.gauge.value(key.clone())),
                Facet::TimingPercentile(ref key) => {
                    if let Some(hs) = self.histogram.snapshot(key.clone()) {
                        snapshot.set_timing_percentiles(key.clone(), hs, &self.percentiles)
                    }
                },
                Facet::ValuePercentile(ref key) => {
                    if let Some(hs) = self.histogram.snapshot(key.clone()) {
                        snapshot.set_value_percentiles(key.clone(), hs, &self.percentiles)
                    }
                },
            }
        }

        snapshot.into_inner()
    }

    fn process_control_msg(&mut self, msg: ControlMessage<ScopedKey<T>>) {
        match msg {
            ControlMessage::AddFacet(facet) => self.add_facet(facet),
            ControlMessage::RemoveFacet(facet) => self.remove_facet(facet),
            ControlMessage::Snapshot(tx) => {
                let snapshot = self.get_snapshot();
                let _ = tx.send(snapshot);
            },
        }
    }

    /// Run the receiver.
    pub fn run(&mut self) {
        let upkeep_rx = tick(Duration::from_millis(100));
        loop {
            if upkeep_rx.try_recv().is_ok() {
                let now = Instant::now();
                self.histogram.upkeep(now);
            }

            while let Ok(msg) = self.control_rx.try_recv() {
                self.process_control_msg(msg);
            }

            match self.data_rx.try_recv() {
                Ok(item) => {
                    self.counter.update(&item);
                    self.gauge.update(&item);
                    self.histogram.update(&item);
                },
                Err(TryRecvError::Empty) => {},
                Err(e) => eprintln!("error receiving data message: {}", e),
            }
        }
    }

    /// Registers a facet with the receiver.
    fn add_facet(&mut self, facet: Facet<ScopedKey<T>>) {
        match facet.clone() {
            Facet::Count(t) => self.counter.register(t),
            Facet::Gauge(t) => self.gauge.register(t),
            Facet::TimingPercentile(t) => self.histogram.register(t),
            Facet::ValuePercentile(t) => self.histogram.register(t),
        }

        self.facets.insert(facet);
    }

    /// Deregisters a facet from the receiver.
    fn remove_facet(&mut self, facet: Facet<ScopedKey<T>>) {
        match facet.clone() {
            Facet::Count(t) => self.counter.deregister(t),
            Facet::Gauge(t) => self.gauge.deregister(t),
            Facet::TimingPercentile(t) => self.histogram.deregister(t),
            Facet::ValuePercentile(t) => self.histogram.deregister(t),
        }

        self.facets.remove(&facet);
    }
}
