use crate::{
    configuration::Configuration,
    control::{ControlFrame, Controller},
    data::{
        default_percentiles, Counter, Facet, Gauge, Histogram, Percentile, Sample, ScopedKey, Snapshot,
        SnapshotBuilder, StringScopedKey,
    },
    sink::Sink,
};
use crossbeam_channel::{self, bounded, tick, TryRecvError};
use quanta::Clock;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    hash::Hash,
    sync::atomic::{AtomicUsize, Ordering, ATOMIC_USIZE_INIT},
    time::{Duration, Instant},
};

static GLOBAL_SCOPE_ID: AtomicUsize = ATOMIC_USIZE_INIT;

pub(crate) fn get_scope_id() -> usize {
    loop {
        let value = GLOBAL_SCOPE_ID.fetch_add(1, Ordering::SeqCst);
        if value != 0 {
            // 0 is reserved.
            return value;
        }
    }
}

/// Wrapper for all messages that flow over the data channel between sink/receiver.
pub(crate) enum MessageFrame<T> {
    /// A normal data message holding a metric sample.
    Data(Sample<T>),

    /// An upkeep message.
    ///
    /// This includes registering/deregistering facets, and registering scopes.
    Upkeep(UpkeepMessage<T>),
}

/// Various upkeep actions performed by a sink.
pub(crate) enum UpkeepMessage<T> {
    /// Registers a new facet with the receiver.
    AddFacet(Facet<T>),

    /// Deregisters an existing facet from the receiver.
    RemoveFacet(Facet<T>),

    /// Registers a scope ID/scope identifier pair.
    RegisterScope(usize, String),
}

/// Metrics receiver which aggregates and processes samples.
pub struct Receiver<T: Clone + Eq + Hash + Display + Send> {
    // Sample aggregation machinery.
    msg_tx: crossbeam_channel::Sender<MessageFrame<ScopedKey<T>>>,
    msg_rx: crossbeam_channel::Receiver<MessageFrame<ScopedKey<T>>>,
    control_tx: crossbeam_channel::Sender<ControlFrame>,
    control_rx: crossbeam_channel::Receiver<ControlFrame>,
    facets: HashSet<Facet<ScopedKey<T>>>,

    // Metric machinery.
    counter: Counter<ScopedKey<T>>,
    gauge: Gauge<ScopedKey<T>>,
    histogram: Histogram<ScopedKey<T>>,
    percentiles: Vec<Percentile>,

    clock: Clock,
    scopes: HashMap<usize, String>,
}

impl<T: Clone + Eq + Hash + Display + Send> Receiver<T> {
    pub(crate) fn from_config(conf: Configuration<T>) -> Receiver<T> {
        // Create our data, control, and buffer channels.
        let (msg_tx, msg_rx) = bounded(conf.capacity);
        let (control_tx, control_rx) = bounded(16);

        Receiver {
            msg_tx,
            msg_rx,
            control_tx,
            control_rx,
            facets: HashSet::new(),
            counter: Counter::new(),
            gauge: Gauge::new(),
            histogram: Histogram::new(Duration::from_secs(10), Duration::from_secs(1)),
            percentiles: default_percentiles(),
            clock: Clock::new(),
            scopes: HashMap::new(),
        }
    }

    /// Gets a builder to configure a `Receiver` instance with.
    pub fn builder() -> Configuration<T> { Configuration::default() }

    /// Creates a `Sink` bound to this receiver.
    pub fn get_sink(&self) -> Sink<T> { Sink::new(self.msg_tx.clone(), self.clock.clone(), "".to_owned(), 0) }

    /// Creates a `Controller` bound to this receiver.
    pub fn get_controller(&self) -> Controller { Controller::new(self.control_tx.clone()) }

    /// Run the receiver.
    pub fn run(&mut self) {
        let upkeep_rx = tick(Duration::from_millis(100));

        loop {
            if upkeep_rx.try_recv().is_ok() {
                let now = Instant::now();
                self.histogram.upkeep(now);
            }

            while let Ok(cframe) = self.control_rx.try_recv() {
                self.process_control_frame(cframe);
            }

            match self.msg_rx.try_recv() {
                Ok(mframe) => self.process_msg_frame(mframe),
                Err(TryRecvError::Empty) => {},
                Err(e) => eprintln!("error receiving message frame: {}", e),
            }
        }
    }

    /// Gets the string representation of an integer scope.
    ///
    /// Returns `Some(scope)` if found, `None` otherwise.  Scope ID `0` is reserved for the root
    /// scope.
    fn get_string_scope(&self, key: ScopedKey<T>) -> Option<StringScopedKey<T>> {
        let scope_id = key.id();
        if scope_id == 0 {
            return Some(key.into_string_scoped("".to_owned()));
        }

        self.scopes
            .get(&scope_id)
            .map(|scope| key.into_string_scoped(scope.clone()))
    }

    /// Gets a snapshot of the current metrics/facets.
    fn get_snapshot(&mut self) -> Snapshot {
        let mut snapshot = SnapshotBuilder::new();
        for facet in &self.facets {
            match *facet {
                Facet::Count(ref key) => {
                    if let Some(actual_key) = self.get_string_scope(key.clone()) {
                        snapshot.set_count(actual_key, self.counter.value(key));
                    }
                },
                Facet::Gauge(ref key) => {
                    if let Some(actual_key) = self.get_string_scope(key.clone()) {
                        snapshot.set_value(actual_key, self.gauge.value(key));
                    }
                },
                Facet::TimingPercentile(ref key) => {
                    if let Some(hs) = self.histogram.snapshot(key) {
                        if let Some(actual_key) = self.get_string_scope(key.clone()) {
                            snapshot.set_timing_percentiles(actual_key, hs, &self.percentiles);
                        }
                    }
                },
                Facet::ValuePercentile(ref key) => {
                    if let Some(hs) = self.histogram.snapshot(key) {
                        if let Some(actual_key) = self.get_string_scope(key.clone()) {
                            snapshot.set_value_percentiles(actual_key, hs, &self.percentiles);
                        }
                    }
                },
            }
        }

        snapshot.into_inner()
    }

    /// Processes a control frame.
    fn process_control_frame(&mut self, msg: ControlFrame) {
        match msg {
            ControlFrame::Snapshot(tx) => {
                let snapshot = self.get_snapshot();
                let _ = tx.send(snapshot);
            },
        }
    }

    /// Processes an upkeep message.
    fn process_upkeep_msg(&mut self, msg: UpkeepMessage<ScopedKey<T>>) {
        match msg {
            UpkeepMessage::AddFacet(facet) => self.add_facet(facet),
            UpkeepMessage::RemoveFacet(facet) => self.remove_facet(facet),
            UpkeepMessage::RegisterScope(id, scope) => {
                let _ = self.scopes.entry(id).or_insert_with(|| scope);
            },
        }
    }

    /// Processes a message frame.
    fn process_msg_frame(&mut self, msg: MessageFrame<ScopedKey<T>>) {
        match msg {
            MessageFrame::Upkeep(umsg) => self.process_upkeep_msg(umsg),
            MessageFrame::Data(sample) => {
                let sample = match sample {
                    Sample::Timing(key, start, end, _) => {
                        let delta = self.clock.delta(start, end);

                        Sample::Value(key, delta)
                    },
                    x => x,
                };

                self.counter.update(&sample);
                self.gauge.update(&sample);
                self.histogram.update(&sample);
            },
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
            Facet::Count(t) => self.counter.deregister(&t),
            Facet::Gauge(t) => self.gauge.deregister(&t),
            Facet::TimingPercentile(t) => self.histogram.deregister(&t),
            Facet::ValuePercentile(t) => self.histogram.deregister(&t),
        }

        self.facets.remove(&facet);
    }
}
