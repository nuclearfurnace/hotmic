use crate::clock::{ClockSource, ClockType};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Mock {
    offset: AtomicUsize,
}

impl Mock {
    pub fn new(offset: usize) -> Self {
        Self {
            offset: AtomicUsize::new(offset),
        }
    }

    pub fn increment(&self, amount: usize) { self.offset.fetch_add(amount, Ordering::Release); }
}

impl ClockSource for Mock {
    fn clock_type(&self) -> ClockType { ClockType::Monotonic }

    fn now(&self) -> u64 { self.offset.load(Ordering::Acquire) as u64 }

    fn start(&self) -> u64 { self.offset.load(Ordering::Acquire) as u64 }

    fn end(&self) -> u64 { self.offset.load(Ordering::Acquire) as u64 }
}
