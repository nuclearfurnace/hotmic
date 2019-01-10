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

pub(crate) trait ClockSource {
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
pub(crate) enum ClockType {
    Monotonic,
    Counter,
}

#[derive(Clone)]
pub(crate) struct Calibration {
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

/// High-speed timing facility.
///
/// [`Clock`] provides a generalized interface to native high-speed timing mechanisms on different
/// target platforms, including support for accessing the TSC (time stamp counter) directly from
/// the processor.
///
/// # Design
///
/// Internally, two clocks are used — a reference and a source — to provide high-speed access to
/// timing values, while allowing for conversion back to a reference timescale that matches the
/// underlying system.
///
/// Calibration between the reference and source happens at initialization time of [`Clock`].
///
/// # Platform support
///
/// [`Clock`] supports using the native high-speed timing facilities on the following platforms:
/// - Windows ([QueryPerformanceCounter])
/// - macOS/OS X ([mach_absolute_time])
/// - Linux/*BSD/Solaris ([clock_gettime])
///
/// # TSC support
///
/// Additionally, [`Clock`] has `RDTSC`/`RDTSCP` support for querying the time stamp counter directly
/// from the processor.  Using this mode has some caveats:
/// - you need to be running nightly to have access to the `asm!` macro/feature ([#29722])
/// - your processor needs to be recent enough to support a stable TSC mode like invariant or
/// constant TSC ([source][tsc_support])
///
/// This feature is only usable when compiled with the `asm` feature flag.
///
/// Generally speaking, most modern operating systems will already be attempting to use the TSC on
/// your behalf, along with switching to another clocksource if they determine that the TSC is
/// unstable or providing unsuitable speed/accuracy.  The primary reason to use TSC directly is that
/// calling it from userspace is measurably faster — although the "slower" methods are still on
/// the order of of tens of nanoseconds — and can be useful for timing operations which themselves
/// are _extremely_ fast.
///
/// If your operations are in the hundreds of nanoseconds range, or you're measuring in a tight
/// loop, using the TSC support could help you avoid the normal overhead which would otherwise
/// contribute to a large chunk of actual time spent and would otherwise consume valuable cycles.
///
/// [QueryPerformanceCounter]: https://msdn.microsoft.com/en-us/library/ms644904(v=VS.85).aspx
/// [mach_absolute_time]: https://developer.apple.com/library/archive/qa/qa1398/_index.html
/// [clock_gettime]: https://linux.die.net/man/3/clock_gettime
/// [#29722]: https://github.com/rust-lang/rust/issues/29722
/// [tsc_support]: http://oliveryang.net/2015/09/pitfalls-of-TSC-usage/
#[derive(Clone, Default)]
pub struct Clock {
    reference: Reference,
    source: Source,
    cal: Calibration,
}

impl Clock {
    /// Creates a [`Clock`] with the optimal reference and source.
    pub fn new() -> Clock { Clock::from_parts(Reference::new(), Source::new(), Calibration::new()) }

    pub(crate) fn from_parts(r: Reference, s: Source, mut calibration: Calibration) -> Clock {
        if !calibration.calibrated() {
            calibration.calibrate(&r, &s);
        }

        Clock {
            reference: r,
            source: s,
            cal: calibration,
        }
    }

    /// Gets the current time, scaled to reference time.
    ///
    /// Value is in nanoseconds.
    ///
    /// This method is not recomended for use with measurements that will be sent in metrics as the
    /// value is pre-scaled and equivalent functionality (with higher performance) can be acheieved
    /// by calling [`raw`] instead.
    ///
    /// Only use this method if you need a high-speed method of getting the current time.
    ///
    /// [`raw`]: Clock::raw
    pub fn now(&self) -> u64 { self.scaled(self.source.now()) }

    /// Gets the underlying time from the source clock.
    ///
    /// Value is not guaranteed to be in nanoseconds.
    ///
    /// This is the fastest way to store the current time when measuring an operation that will be
    /// sent later as a metric.  It requires conversion to reference time, however, via [`scaled`] or
    /// [`delta`].
    ///
    /// If you need maximum accuracy in your measurements, consider using [`start`] and [`end`].
    ///
    /// [`scaled`]: Clock::scaled
    /// [`delta`]: Clock::delta
    /// [`start`]: Clock::start
    /// [`end`]: Clock::end
    pub fn raw(&self) -> u64 { self.source.now() }

    /// Gets the underlying time from the source clock, specific to starting an operation.
    ///
    /// Provides the same functionality as [`raw`], but tries to ensure that no extra CPU
    /// instructions end up executing after the measurement is taken.  Since normal processors are
    /// typically out-of-order, other operations that logically come before a call to this method
    /// could be reordered to come after the measurement, thereby skewing the overall time
    /// measured.
    ///
    /// [`raw`]: Clock::raw
    pub fn start(&self) -> u64 { self.source.start() }

    /// Gets the underlying time from the source clock, specific to ending an operation.
    ///
    /// Provides the same functionality as [`raw`], but tries to ensure that no extra CPU
    /// instructions end up executing before the measurement is taken.  Since normal processors are
    /// typically out-of-order, other operations that logically come after a call to this method
    /// could be reordered to come before the measurement, thereby skewing the overall time
    /// measured.
    ///
    /// [`raw`]: Clock::raw
    pub fn end(&self) -> u64 { self.source.end() }

    /// Scales a raw measurement to reference time.
    ///
    /// Value is in nanoseconds.
    pub fn scaled(&self, value: u64) -> u64 {
        (((value as f64 - self.cal.src_time) * self.cal.hz_ratio) + self.cal.ref_time) as u64
    }

    /// Calculates the delta between two measurements, and scales to reference time.
    ///
    /// Value is in nanoseconds.
    ///
    /// This method is the fastest way to get the delta between two raw measurements, or a
    /// start/end measurement pair, where it is also scaled to reference time.
    pub fn delta(&self, start: u64, end: u64) -> u64 { (end.wrapping_sub(start) as f64 * self.cal.hz_ratio) as u64 }
}
