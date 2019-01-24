use crate::{
    data::{Sample, ScopedKey},
    helper::io_error,
    receiver::{get_scope_id, MessageFrame, UpkeepMessage},
};
use crossbeam_channel::Sender;
use quanta::Clock;
use std::{fmt::Display, hash::Hash};

/// Erorrs during sink creation.
#[derive(Debug)]
pub enum SinkError {
    /// The scope value given was invalid i.e. empty or illegal characters.
    InvalidScope,
}

/// Handle for sending metric samples into the receiver.
///
/// [`Sink`] is cloneable, and can not only send metric samples but can register and deregister
/// metric facets at any time.
pub struct Sink<T: Clone + Eq + Hash + Display> {
    msg_tx: Sender<MessageFrame<ScopedKey<T>>>,
    clock: Clock,
    scope: String,
    scope_id: usize,
}

impl<T: Clone + Eq + Hash + Display> Sink<T> {
    pub(crate) fn new(
        msg_tx: Sender<MessageFrame<ScopedKey<T>>>, clock: Clock, scope: String, scope_id: usize,
    ) -> Sink<T> {
        // Register our scope with the receiver.  We send this over the data channel because the
        // control channel is usually smaller, so we have a better chance of not deadlocking
        // ourselves by creating many sinks before the receiver is started, alsooooo, we could get
        // into a situation where a sink sent metrics that got processed before the scope was
        // registered depending on when it was creating during a single turn of the receiver.
        //
        // Long story short, we need to make sure we register the scope before processing any data
        // samples, so we shove them both into the same channel to get that guarantee.
        let umsg = UpkeepMessage::RegisterScope(scope_id, scope.clone());
        let _ = msg_tx.send(MessageFrame::Upkeep(umsg));

        Sink {
            msg_tx,
            clock,
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
    pub fn scoped(&self, scope: &str) -> Result<Sink<T>, SinkError> {
        if scope.is_empty() {
            return Err(SinkError::InvalidScope);
        }

        let mut new_scope = self.scope.clone();
        if !new_scope.is_empty() {
            new_scope.push('.');
        }
        new_scope.push_str(scope);

        Ok(Sink::new(
            self.msg_tx.clone(),
            self.clock.clone(),
            new_scope,
            get_scope_id(),
        ))
    }

    /// Reference to the internal high-speed clock interface.
    pub fn clock(&self) -> &Clock { &self.clock }

    /// Updates the count for a given metric.
    pub fn update_count(&self, key: T, delta: i64) { self.send(Sample::Count(key, delta)) }

    /// Updates the value for a given metric.
    ///
    /// This can be used either for setting a gauge or updating a value histogram.
    pub fn update_gauge(&self, key: T, value: u64) { self.send(Sample::Gauge(key, value)) }

    /// Updates the timing histogram for a given metric.
    pub fn update_timing(&self, key: T, start: u64, end: u64) { self.send(Sample::TimingHistogram(key, start, end, 1)) }

    /// Updates the timing histogram for a given metric, with a count.
    pub fn update_timing_with_count(&self, key: T, start: u64, end: u64, count: u64) {
        self.send(Sample::TimingHistogram(key, start, end, count))
    }

    /// Updates the value histogram for a given metric.
    pub fn update_value(&self, key: T, value: u64) { self.send(Sample::ValueHistogram(key, value)) }

    /// Increments the given metric by one.
    pub fn increment(&self, key: T) { self.update_count(key, 1) }

    /// Decrements the given metric by one.
    pub fn decrement(&self, key: T) { self.update_count(key, -1) }

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
            scope: self.scope.clone(),
            scope_id: self.scope_id,
        }
    }
}
