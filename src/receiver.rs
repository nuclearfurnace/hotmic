use crate::{
    configuration::Configuration,
    control::{ControlFrame, Controller},
    data::{Counter, Gauge, Histogram, Sample, ScopedKey, Snapshot, SnapshotBuilder, StringScopedKey},
    sink::Sink,
};
use crossbeam_channel::{self, bounded, TryRecvError};
use quanta::Clock;
use std::{
    collections::HashMap,
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
    Upkeep(UpkeepMessage),
}

/// Various upkeep actions performed by a sink.
pub(crate) enum UpkeepMessage {
    /// Registers a scope ID/scope identifier pair.
    RegisterScope(usize, String),
}

/// Metrics receiver which aggregates and processes samples.
pub struct Receiver<T: Clone + Eq + Hash + Display + Send> {
    config: Configuration<T>,

    // Sample aggregation machinery.
    msg_tx: crossbeam_channel::Sender<MessageFrame<ScopedKey<T>>>,
    msg_rx: crossbeam_channel::Receiver<MessageFrame<ScopedKey<T>>>,
    control_tx: crossbeam_channel::Sender<ControlFrame>,
    control_rx: crossbeam_channel::Receiver<ControlFrame>,

    // Metric machinery.
    counter: Counter<ScopedKey<T>>,
    gauge: Gauge<ScopedKey<T>>,
    thistogram: Histogram<ScopedKey<T>>,
    vhistogram: Histogram<ScopedKey<T>>,

    clock: Clock,
    scopes: HashMap<usize, String>,
}

impl<T: Clone + Eq + Hash + Display + Send> Receiver<T> {
    pub(crate) fn from_config(config: Configuration<T>) -> Receiver<T> {
        // Create our data, control, and buffer channels.
        let (msg_tx, msg_rx) = bounded(config.capacity);
        let (control_tx, control_rx) = bounded(16);

        let histogram_window = config.histogram_window;
        let histogram_granularity = config.histogram_granularity;

        Receiver {
            config,
            msg_tx,
            msg_rx,
            control_tx,
            control_rx,
            counter: Counter::new(),
            gauge: Gauge::new(),
            thistogram: Histogram::new(histogram_window, histogram_granularity),
            vhistogram: Histogram::new(histogram_window, histogram_granularity),
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
        let batch_size = self.config.batch_size;
        let mut next_upkeep = self.clock.now() + duration_as_nanos(Duration::from_millis(250));
        let mut batch = Vec::with_capacity(batch_size);

        loop {
            if self.clock.now() > next_upkeep {
                let now = Instant::now();
                self.thistogram.upkeep(now);
                self.vhistogram.upkeep(now);
                next_upkeep = self.clock.now() + duration_as_nanos(Duration::from_millis(250));
            }

            while let Ok(cframe) = self.control_rx.try_recv() {
                self.process_control_frame(cframe);
            }

            loop {
                match self.msg_rx.try_recv() {
                    Ok(mframe) => batch.push(mframe),
                    Err(TryRecvError::Empty) => break,
                    Err(e) => eprintln!("error receiving message frame: {}", e),
                }

                if batch.len() == batch_size {
                    break;
                }
            }

            if !batch.is_empty() {
                for mframe in batch.drain(0..) {
                    self.process_msg_frame(mframe);
                }
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
        let cvalues = self.counter.values();
        let gvalues = self.gauge.values();
        let tvalues = self.thistogram.values();
        let vvalues = self.vhistogram.values();

        for (key, value) in cvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_count(actual_key, value);
            }
        }

        for (key, value) in gvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_gauge(actual_key, value);
            }
        }

        for (key, value) in tvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_timing_histogram(actual_key, value, &self.config.percentiles);
            }
        }

        for (key, value) in vvalues {
            if let Some(actual_key) = self.get_string_scope(key) {
                snapshot.set_value_histogram(actual_key, value, &self.config.percentiles);
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
    fn process_upkeep_msg(&mut self, msg: UpkeepMessage) {
        match msg {
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
                match sample {
                    Sample::Count(key, count) => {
                        self.counter.update(key, count);
                    },
                    Sample::Gauge(key, value) => {
                        self.gauge.update(key, value);
                    },
                    Sample::TimingHistogram(key, start, end, count) => {
                        let delta = self.clock.delta(start, end);
                        self.counter.update(key.clone(), count as i64);
                        self.thistogram.update(key, delta);
                    },
                    Sample::ValueHistogram(key, value) => {
                        self.vhistogram.update(key, value);
                    },
                }
            },
        }
    }
}

fn duration_as_nanos(d: Duration) -> u64 { (d.as_secs() * 1_000_000_000) + u64::from(d.subsec_nanos()) }
