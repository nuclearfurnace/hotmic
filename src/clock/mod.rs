use std::sync::Arc;

mod monotonic;
pub use self::monotonic::Monotonic;
mod counter;
pub use self::counter::Counter;
mod mock;
pub use self::mock::Mock;

type Reference = Monotonic;

#[cfg(feature = "tsc")]
type Source = Counter;
#[cfg(not(feature = "tsc"))]
type Source = Monotonic;

pub trait ClockSource {
    fn clock_type(&self) -> ClockType;
    fn now(&self) -> u64;
    fn start(&self) -> u64;
    fn end(&self) -> u64;
}

impl<T: ClockSource> ClockSource for Arc<T> {
    fn clock_type(&self) -> ClockType { (**self).clock_type() }

    fn now(&self) -> u64 { (**self).now() }

    fn start(&self) -> u64 { (**self).start() }

    fn end(&self) -> u64 { (**self).end() }
}

#[derive(Clone, Copy, PartialEq)]
pub enum ClockType {
    Monotonic,
    Counter,
}

#[derive(Clone)]
pub struct Calibration {
    calibrated: bool,
    ref_time: f64,
    src_time: f64,
    ref_hz: f64,
    src_hz: f64,
    hz_ratio: f64,
}

impl Calibration {
    pub fn new() -> Calibration {
        Calibration {
            calibrated: false,
            ref_time: 0.0,
            src_time: 0.0,
            ref_hz: 1_000_000_000.0,
            src_hz: 1_000_000_000.0,
            hz_ratio: 1.0,
        }
    }

    pub fn calibrated(&self) -> bool { self.calibrated }

    pub fn calibrate<R, S>(&mut self, reference: &R, source: &S)
    where
        R: ClockSource,
        S: ClockSource,
    {
        if reference.clock_type() == source.clock_type() {
            self.calibrated = true;
            return;
        }

        self.ref_time = reference.now() as f64;
        self.src_time = source.start() as f64;

        let ref_end = self.ref_time + self.ref_hz;

        loop {
            let t = reference.now() as f64;
            if t >= ref_end {
                break;
            }
        }

        let src_end = source.end() as f64;

        let ref_d = ref_end - self.ref_time;
        let src_d = src_end - self.src_time;

        self.src_hz = (src_d * self.ref_hz) / ref_d;
        self.hz_ratio = self.ref_hz / self.src_hz;
        self.calibrated = true;
    }
}

impl Default for Calibration {
    fn default() -> Self { Self::new() }
}

#[derive(Clone, Default)]
pub struct Clock {
    reference: Reference,
    source: Source,
    cal: Calibration,
}

impl Clock {
    pub fn new() -> Clock { Clock::from_parts(Reference::new(), Source::new(), Calibration::new()) }

    pub fn from_parts(r: Reference, s: Source, mut calibration: Calibration) -> Clock {
        if !calibration.calibrated() {
            calibration.calibrate(&r, &s);
        }

        Clock {
            reference: r,
            source: s,
            cal: calibration,
        }
    }

    pub fn now(&self) -> u64 {
        let value = self.source.now();
        if self.reference.clock_type() == self.source.clock_type() {
            return value;
        }

        (((value as f64 - self.cal.src_time) * self.cal.hz_ratio) + self.cal.ref_time) as u64
    }

    pub fn raw(&self) -> u64 { self.source.now() }

    pub fn start(&self) -> u64 { self.source.start() }

    pub fn end(&self) -> u64 { self.source.end() }

    pub fn delta(&self, start: u64, end: u64) -> u64 { (end.wrapping_sub(start) as f64 * self.cal.hz_ratio) as u64 }
}
