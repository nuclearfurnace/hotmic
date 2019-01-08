use crate::clock::{ClockSource, ClockType};

#[derive(Clone, Default)]
pub struct Counter;

impl Counter {
    pub fn new() -> Self { Counter {} }
}

#[cfg(feature = "tsc")]
impl ClockSource for Counter {
    fn clock_type(&self) -> ClockType { ClockType::Counter }

    fn now(&self) -> u64 {
        let mut l: u32;
        let mut h: u32;
        unsafe {
            asm!("lfence; rdtsc" : "={eax}" (l), "={edx}" (h) ::: "volatile");
        }
        ((h as u64) << 32) | l as u64
    }

    fn start(&self) -> u64 {
        let mut l: u32;
        let mut h: u32;
        unsafe {
            asm!("lfence; rdtsc; lfence" : "={eax}" (l), "={edx}" (h) ::: "volatile");
        }
        ((h as u64) << 32) | l as u64
    }

    fn end(&self) -> u64 {
        let mut l: u32;
        let mut h: u32;
        unsafe {
            asm!("rdtscp; lfence" : "={eax}" (l), "={edx}" (h) ::: "volatile");
        }
        ((h as u64) << 32) | l as u64
    }
}

#[cfg(not(feature = "tsc"))]
impl ClockSource for Counter {
    fn clock_type(&self) -> ClockType { ClockType::Counter }

    fn now(&self) -> u64 {
        panic!("can't use counter without TSC support");
    }

    fn start(&self) -> u64 {
        panic!("can't use counter without TSC support");
    }

    fn end(&self) -> u64 {
        panic!("can't use counter without TSC support");
    }
}
