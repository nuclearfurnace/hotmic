//! High-speed metrics collection library.
//!
//! hotmic provides a generalized metrics collection library targeted at users who want to log
//! metrics at high volume and high speed.
//!
//! # Design
//!
//! The library follows a pattern of "senders" and a "receiver."
//!
//! Callers create a [`Receiver`], which acts as a contained unit: metric registration,
//! aggregation, and summarization.  The [`Receiver`] is intended to be spawned onto a dedicated
//! background thread.
//!
//! From a [`Receiver`], callers can create a [`Sink`], which allows registering facets -- or
//! interests -- in a given metric, along with sending the metrics themselves.  All metrics need to
//! be pre-registered, in essence, with the receiver, which allows us to know which aspects of a
//! metric to track: count, value, or percentile.
//!
//! A [`Sink`] can be cheaply cloned and does not require a mutable reference to send metrics, and
//! so callers have great flexibility in being able to control their resource consumption when it
//! comes to sinks. [`Receiver`] also allows configuring the capacity of the underlying channels to
//! finely tune resource consumption.
//!
//! Being based on [`crossbeam-channel`] allows us to process close to five million metrics per second
//! on a single core, with very low ingest latencies: 325-350ns on average at full throughput.
//!
//! # Metrics
//!
//! hotmic supports counters, gauges, and histograms.
//!
//! A counter is a single value that can be updated with deltas to increase or decrease the value.
//! This would be your typical "messages sent" or "database queries executed" style of metric,
//! where the value changes over time.
//!
//! A gauge is also a single value but does not support delta updates.  When a gauge is set, the
//! value sent becomes _the_ value of the gauge.  Gauges can be useful for metrics that measure a
//! point-in-time value, such as "connected clients" or "running queries".  While those metrics
//! could also be represented by a count, gauges can be simpler in cases where you're already
//! computing and storing the value, and simply want to expose it in your metrics.
//!
//! A histogram tracks the distribution of values: how many values were between 0-5, between 6-10,
//! etc.  This is the canonical way to measure latency: the time spent running a piece of code or
//! servicing an operation.  By keeping track of the individual measurements, we can better see how
//! many are slow, fast, average, and in what proportions.
//!
//! ```
//! # extern crate hotmic;
//! use hotmic::{Facet, Receiver};
//! use std::thread;
//! let receiver = Receiver::builder().build();
//! let sink = receiver.get_sink();
//!
//! // We have to register the metrics we care about so that they're properly tracked!
//! sink.add_facet(Facet::Count("widget"));
//! sink.add_facet(Facet::Gauge("red_balloons"));
//! sink.add_facet(Facet::TimingPercentile("db.gizmo_query"));
//! sink.add_facet(Facet::Count("db.gizmo_query"));
//! sink.add_facet(Facet::ValuePercentile("buf_size"));
//!
//! // We can send a simple count, which is a signed value, so the value we give is applied as a
//! // delta to the underlying counter.  After these sends, "widgets" would be 3.
//! assert!(sink.update_count("widgets", 5).is_ok());
//! assert!(sink.update_count("widgets", -3).is_ok());
//! assert!(sink.update_count("widgets", 1).is_ok());
//!
//! // We can update a gauge.  This is just a point-in-time value so the last "write" of this
//! // metric is what the value will be, and it will stay at that value until changed.
//! assert!(sink.update_value("red_balloons", 99).is_ok());
//!
//! // We can update a timing percentile.  For timing, you also must measure the start and end
//! // time using the built-in `Clock` exposed by the sink.  The receiver internally converts the
//! // raw values to calculate the actual wall clock time (in nanoseconds) on your behalf, so you
//! // can't just pass in any old number.. otherwise you'll get erroneous measurements!
//! let start = sink.clock().start();
//! thread::sleep_ms(10);
//! let end = sink.clock().end();
//! let rows = 42;
//!
//! // This would just set the timing:
//! assert!(sink.update_timing("db.gizmo_query", start, end).is_ok());
//!
//! // This would set the timing and also let you provide a customized count value.  Being able to
//! // specify a count is handy when tracking things like the time it took to execute a database
//! // query, along with how many rows that query returned:
//! assert!(sink
//!     .update_timing_with_count("db.gizmo_query", start, end, rows)
//!     .is_ok());
//!
//! // Finally, we can update a value percentile.  Technically speaking, value percentiles aren't
//! // fundamentally different from timing percentiles.  If you use a timing percentile, we do the
//! // math for you of getting the time difference, and we make sure the metric name has the right
//! // unit suffix so you can tell it's measuring time, but other than that, nearly identical!
//! let buf_size = 4096;
//! assert!(sink.update_value("buf_size", buf_size).is_ok());
//! ```
//!
//! # Facets
//!
//! Facets are the way callers specify what they're interested in.  Without any other
//! configuration, you could send any metric you want but nothing would happen; nothing would be
//! recorded.
//!
//! Facets correspond roughly to the metric types, with the exception of the difference between
//! timing percentiles and value percentiles, which both are histogram-based but differ in how we
//! render their metric labels.
//!
//! Thus, if you want to record a counter, you would register a counter facet for the given metric
//! key, and if you want to track latency for a given operation, you would register a timing
//! percentile for the metric key used.
//!
//! Facets and scoping (explained below) are intrinsically tied together, so facets need to be
//! registered directly on the sink they'll be used from in order to ensure that the facet matches
//! the scope of the sink:
//!
//! ```
//! # extern crate hotmic;
//! use hotmic::{Facet, Receiver};
//! let receiver = Receiver::builder().build();
//!
//! // This sink has no scope aka the root scope.  We can register facets on this sink without a
//! // problem, but if get a scoped sink from this one, and sent the same metric name, the scopes
//! // would not line up, and the metric wouldn't be registered for storage.
//! let root_sink = receiver.get_sink();
//! root_sink.add_facet(Facet::Count("widgets"));
//! assert!(root_sink.update_count("widgets", 42).is_ok());
//!
//! // Make a new scoped sink.  If we tried to send to this new sink, without reregistering our
//! // facets, our metrics wouldn't be stored at all.
//! let scoped_sink = root_sink.scoped("party").unwrap();
//!
//! // Register the facet, and we're all good.
//! scoped_sink.add_facet(Facet::Count("widgets"));
//! assert!(scoped_sink.update_count("widgets", 43).is_ok());
//! ```
//!
//! # Scopes
//!
//! Metrics can be scoped, not unlike loggers, at the [`Sink`] level.  This allows sinks to easily
//! nest themselves without callers ever needing to care about where they're located.
//!
//! This feature is a simpler approach to tagging: while not as semantically rich, it provides the
//! level of detail necessary to distinguish a single metric between multiple callsites.
//!
//! For example, after getting a [`Sink`] from the [`Receiver`], we can easily nest ourselves under
//! the root scope and then send some metrics:
//!
//! ```
//! # extern crate hotmic;
//! use hotmic::Receiver;
//! let receiver = Receiver::builder().build();
//!
//! // This sink has no scope aka the root scope.  The metric will just end up as "widgets".
//! let root_sink = receiver.get_sink();
//! assert!(root_sink.update_count("widgets", 42).is_ok());
//!
//! // This sink is under the "secret" scope.  Since we derived ourselves from the root scope,
//! // we're not nested under anything, but our metric name will end up being "secret.widgets".
//! let scoped_sink = root_sink.scoped("secret").unwrap();
//! assert!(scoped_sink.update_count("widgets", 42).is_ok());
//!
//! // This sink is under the "supersecret" scope, but we're also nested!  The metric name for this
//! // sample will end up being "secret.supersecret.widget".
//! let scoped_sink_two = scoped_sink.scoped("supersecret").unwrap();
//! assert!(scoped_sink_two.update_count("widgets", 42).is_ok());
//!
//! // Sinks retain their scope even when cloned, so the metric name will be the same as above.
//! let cloned_sink = scoped_sink_two.clone();
//! assert!(cloned_sink.update_count("widgets", 42).is_ok());
//! ```
mod configuration;
mod control;
mod data;
mod helper;
mod receiver;
mod sink;

pub use self::{
    configuration::Configuration,
    control::Controller,
    data::{Facet, Percentile, Snapshot},
    receiver::Receiver,
    sink::{Sink, SinkError},
};
