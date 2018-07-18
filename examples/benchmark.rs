#[macro_use]
extern crate log;
extern crate env_logger;
extern crate getopts;
extern crate hotmic;

use std::fmt;
use std::thread;
use std::env;
use std::time::{Instant, Duration};
use getopts::Options;
use hotmic::{Facet, Sample, Source, Sink, Quantile};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Metric {
    Ok,
    Total,
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Metric::Ok => write!(f, "ok"),
            Metric::Total => write!(f, "total"),
        }
    }
}

struct Generator {
    stats: Source<Metric>,
    t0: Option<Instant>,
    gauge: u64,
}

impl Generator {
    fn new(stats: Source<Metric>) -> Generator {
        Generator {
            stats: stats,
            t0: None,
            gauge: 0,
        }
    }

    fn run(&mut self) {
        loop {
            self.gauge += 1;
            let t1 = Instant::now();
            if let Some(t0) = self.t0 {
                let _ = self.stats.send(Sample::Timing(Metric::Ok, t0, t1, 1));
                let _ = self.stats.send(Sample::Value(Metric::Total, self.gauge));
            }
            self.t0 = Some(t1);
        }
    }
}

fn print_usage(program: &str, opts: &Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

pub fn opts() -> Options {
    let mut opts = Options::new();

    opts.optopt("p", "producers", "number of producers", "INTEGER");
    opts.optopt("c", "capacity", "maximum number of unprocessed batches", "INTEGER");
    opts.optopt("b", "batch", "number of samples per source batch", "INTEGER");
    opts.optflag("h", "help", "print this help menu");

    opts
}

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let program = &args[0];
    let opts = opts();

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            error!("Failed to parse command line args: {}", f);
            return;
        }
    };

    if matches.opt_present("help") {
        print_usage(program, &opts);
        return;
    }

    info!("hotmic benchmark");

    // Build our sink and configure the facets.
    let capacity = matches.opt_str("capacity").unwrap_or_else(|| "256".to_owned())
        .parse()
        .unwrap();
    let batch = matches.opt_str("batch").unwrap_or_else(|| "128".to_owned())
        .parse()
        .unwrap();
    let producers = matches.opt_str("producers").unwrap_or_else(|| "1".to_owned())
        .parse()
        .unwrap();

    info!("producers: {}", producers);
    info!("capacity: {}", capacity);
    info!("batch size: {}", batch);

    let mut sink = Sink::builder()
        .capacity(capacity)
        .batch_size(batch)
        .build();

    sink.add_facet(Facet::Count(Metric::Ok));
    sink.add_facet(Facet::TimingPercentile(Metric::Ok));
    sink.add_facet(Facet::Count(Metric::Total));
    sink.add_facet(Facet::Gauge(Metric::Total));

    info!("sink configured");

    // Spin up our sample producers.
    for _ in 0..producers {
        let s = sink.get_source();
        thread::spawn(move || { Generator::new(s).run(); });
    }

    // Spin up the sink and let 'er rip.
    let controller = sink.get_controller();

    thread::spawn(move || { sink.run(); });

    // Poll the controller to figure out the sample rate.
    let mut total = 0;
    let mut t0 = Instant::now();
    for _ in 0..60 {
        let t1 = Instant::now();
        let mut turn_total = 0;

        let snapshot = controller.get_snapshot().unwrap();
        if let Some(t) = snapshot.count(&Metric::Ok) {
            turn_total += *t;
        }

        if let Some(t) = snapshot.count(&Metric::Total) {
            turn_total += *t;
        }

        let turn_delta = turn_total - total;
        total = turn_total;
        let rate = turn_delta as f64 / (duration_as_nanos(t1 - t0) / 1_000_000_000.0);

        info!("rate: {} samples per second", rate);
        info!(
            "latency (ns): p50: {} p90: {} p99: {} p999: {} max: {}",
            snapshot.timing_quantile(&Metric::Ok, Quantile("p50".to_owned(), 0.5))
                .unwrap_or(&0),
            snapshot.timing_quantile(&Metric::Ok, Quantile("p90".to_owned(), 0.9))
                .unwrap_or(&0),
            snapshot.timing_quantile(&Metric::Ok, Quantile("p99".to_owned(), 0.99))
                .unwrap_or(&0),
            snapshot.timing_quantile(&Metric::Ok, Quantile("p999".to_owned(), 0.999))
                .unwrap_or(&0),
            snapshot.timing_quantile(&Metric::Ok, Quantile("max".to_owned(), 1.0))
                .unwrap_or(&0)
        );

        t0 = t1;
        thread::sleep(Duration::new(1, 0));
    }

    info!("total metrics pushed: {}", total);
}

fn duration_as_nanos(d: Duration) -> f64 {
    (d.as_secs() as f64 * 1e9) + d.subsec_nanos() as f64
}
