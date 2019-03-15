use crate::{
    data::{Sample, ScopedKey},
    helper::io_error,
    receiver::MessageFrame,
    scopes::Scopes,
};
use crossbeam_channel::Sender;
use quanta::Clock;
use std::{fmt::Display, hash::Hash, sync::Arc};

/// Erorrs during sink creation.
#[derive(Debug)]
pub enum SinkError {
    /// The scope value given was invalid i.e. empty or illegal characters.
    InvalidScope,
}

/// A value that can be used as a metric scope.
pub trait AsScoped<'a> {
    fn as_scoped(&'a self, base: String) -> String;
}

/// Handle for sending metric samples into the receiver.
///
/// [`Sink`] is cloneable, and can not only send metric samples but can register and deregister
/// metric facets at any time.
pub struct Sink<T: Clone + Eq + Hash + Display> {
    msg_tx: Sender<MessageFrame<ScopedKey<T>>>,
    clock: Clock,
    scopes: Arc<Scopes>,
    scope: String,
    scope_id: u64,
}

impl<T: Clone + Eq + Hash + Display> Sink<T> {
    pub(crate) fn new(
        msg_tx: Sender<MessageFrame<ScopedKey<T>>>,
        clock: Clock,
        scopes: Arc<Scopes>,
        scope: String,
    ) -> Sink<T> {
        let scope_id = scopes.register(scope.clone());

        Sink {
            msg_tx,
            clock,
            scopes,
            scope,
            scope_id,
        }
    }

    pub(crate) fn new_with_scope_id(
        msg_tx: Sender<MessageFrame<ScopedKey<T>>>,
        clock: Clock,
        scopes: Arc<Scopes>,
        scope: String,
        scope_id: u64,
    ) -> Sink<T> {
        Sink {
            msg_tx,
            clock,
            scopes,
            scope,
            scope_id,
        }
    }

    /// Creates a scoped clone of this [`Sink`].
    ///
    /// Scoping controls the resulting metric name for any metrics sent by this [`Sink`].  For
    /// example, you might have a metric called `messages_sent`.
    ///
    /// With scoping, you could have independent versions of the same metric.  This is useful for
    /// having the same "base" metric name but with broken down values.
    ///
    /// Going further with the above example, if you had a server, and listened on multiple
    /// addresses, maybe you would have a scoped [`Sink`] per listener, and could end up with
    /// metrics that look like this:
    /// - `listener.a.messages_sent`
    /// - `listener.b.messages_sent`
    /// - `listener.c.messages_sent`
    /// - etc
    ///
    /// Scopes are also inherited.  If you create a scoped [`Sink`] from another [`Sink`] which is
    /// already scoped, the scopes will be merged together using a `.` as the string separator.
    /// This makes it easy to nest scopes.  Cloning a scoped [`Sink`], though, will inherit the
    /// same scope as the original.
    pub fn scoped<'a, S: AsScoped<'a> + ?Sized>(&self, scope: &'a S) -> Sink<T> {
        let new_scope = scope.as_scoped(self.scope.clone());

        Sink::new(self.msg_tx.clone(), self.clock.clone(), self.scopes.clone(), new_scope)
    }

    /// Reference to the internal high-speed clock interface.
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// Updates the count for a given metric.
    pub fn update_count(&self, key: T, delta: i64) {
        self.send(Sample::Count(key, delta))
    }

    /// Updates the value for a given metric.
    ///
    /// This can be used either for setting a gauge or updating a value histogram.
    pub fn update_gauge(&self, key: T, value: u64) {
        self.send(Sample::Gauge(key, value))
    }

    /// Updates the timing histogram for a given metric.
    pub fn update_timing(&self, key: T, start: u64, end: u64) {
        self.send(Sample::TimingHistogram(key, start, end, 1))
    }

    /// Updates the timing histogram for a given metric, with a count.
    pub fn update_timing_with_count(&self, key: T, start: u64, end: u64, count: u64) {
        self.send(Sample::TimingHistogram(key, start, end, count))
    }

    /// Updates the value histogram for a given metric.
    pub fn update_value(&self, key: T, value: u64) {
        self.send(Sample::ValueHistogram(key, value))
    }

    /// Increments the given metric by one.
    pub fn increment(&self, key: T) {
        self.update_count(key, 1)
    }

    /// Decrements the given metric by one.
    pub fn decrement(&self, key: T) {
        self.update_count(key, -1)
    }

    /// Sends a raw metric sample to the receiver.
    fn send(&self, sample: Sample<T>) {
        let _ = self
            .msg_tx
            .send(MessageFrame::Data(sample.into_scoped(self.scope_id)))
            .map_err(|_| io_error("failed to send sample"));
    }
}

impl<T: Clone + Eq + Hash + Display> Clone for Sink<T> {
    fn clone(&self) -> Sink<T> {
        Sink {
            msg_tx: self.msg_tx.clone(),
            clock: self.clock.clone(),
            scopes: self.scopes.clone(),
            scope: self.scope.clone(),
            scope_id: self.scope_id,
        }
    }
}

impl<'a> AsScoped<'a> for str {
    fn as_scoped(&'a self, mut base: String) -> String {
        if !base.is_empty() {
            base.push_str(".");
        }
        base.push_str(self);
        base
    }
}

impl<'a, 'b, T> AsScoped<'a> for T
where
    &'a T: AsRef<[&'b str]>,
    T: 'a,
{
    fn as_scoped(&'a self, mut base: String) -> String {
        for item in self.as_ref() {
            if !base.is_empty() {
                base.push('.');
            }
            base.push_str(item);
        }
        base
    }
}
