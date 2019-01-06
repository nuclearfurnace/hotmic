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
    fn now(&self) -> u64;
    fn start(&self) -> u64;
    fn end(&self) -> u64;
}

impl<T: ClockSource> ClockSource for Arc<T> {
    fn now(&self) -> u64 {
        (**self).now()
    }

    fn start(&self) -> u64 {
        (**self).start()
    }

    fn end(&self) -> u64 {
        (**self).end()
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum CalibrationMode {
    Identical,
    Calibrated,
    Uncalibrated,
}

#[derive(Clone)]
pub struct Calibration {
    mode: CalibrationMode,
    ref_time: f64,
    src_time: f64,
    ref_hz: f64,
    src_hz: f64,
    hz_ratio: f64,
}

impl Calibration {
    pub fn new() -> Calibration {
        Calibration {
            mode: CalibrationMode::Uncalibrated,
            ref_time: 0.0,
            src_time: 0.0,
            ref_hz: 1_000_000_000.0,
            src_hz: 1_000_000_000.0,
            hz_ratio: 1.0,
        }
    }

    pub fn identical() -> Calibration {
        Calibration {
            mode: CalibrationMode::Identical,
            ref_time: 0.0,
            src_time: 0.0,
            ref_hz: 1.0,
            src_hz: 1.0,
            hz_ratio: 1.0,
        }
    }

    pub fn mode(&self) -> CalibrationMode {
        self.mode
    }

    pub fn calibrate<R, S>(&mut self, reference: &R, source: &S)
    where
        R: ClockSource,
        S: ClockSource,
    {
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
    }
}

#[derive(Clone)]
pub struct Clock {
    reference: Reference,
    source: Source,
    cal: Calibration,
}

impl Clock {
    #[cfg(not(feature = "tsc"))]
    pub fn new() -> Clock {
        Clock::from_parts(Reference::new(), Source::new(), Calibration::identical())
    }

    #[cfg(feature = "tsc")]
    pub fn new() -> Clock {
        Clock::from_parts(Reference::new(), Source::new(), Calibration::new())
    }

    pub fn from_parts(r: Reference, s: Source, mut calibration: Calibration) -> Clock {
        if calibration.mode() == CalibrationMode::Uncalibrated {
            calibration.calibrate(&r, &s);
        }

        Clock {
            reference: r,
            source: s,
            cal: calibration,
        }
    }

    pub fn recalibrate(&mut self) {
        self.cal.calibrate(&self.reference, &self.source);
    }

    pub fn now(&self) -> u64 {
        let value = self.source.now();
        if self.cal.mode == CalibrationMode::Identical {
            return value
        }

        (((value as f64 - self.cal.src_time) * self.cal.hz_ratio) + self.cal.ref_time) as u64
    }

    pub fn raw(&self) -> u64 {
        self.source.now()
    }

    pub fn start(&self) -> u64 {
        self.source.start()
    }

    pub fn end(&self) -> u64 {
        self.source.end()
    }

    pub fn delta(&self, start: u64, end: u64) -> u64 {
        (end.wrapping_sub(start) as f64 * self.cal.hz_ratio) as u64
    }
}
