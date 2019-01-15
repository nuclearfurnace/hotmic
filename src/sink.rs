use crate::{
    control::ControlMessage,
    data::{DataFrame, Facet, Sample, ScopedKey},
    helper::io_error,
    receiver::get_scope_id,
};
use crossbeam_channel::Sender;
use quanta::Clock;
use std::{fmt::Display, hash::Hash, io};

/// Erorrs during sink creation.
#[derive(Debug)]
pub enum SinkError {
    /// The scope value given was invalid i.e. empty or illegal characters.
    InvalidScope,
}

/// Handle for sending metric samples into the receiver.
///
/// `Sink` is cloneable, and can not only send metric samples but can register and deregister
/// metric facets at any time.
pub struct Sink<T: Clone + Eq + Hash + Display> {
    data_tx: Sender<DataFrame<ScopedKey<T>>>,
    control_tx: Sender<ControlMessage<ScopedKey<T>>>,
    clock: Clock,
    scope: String,
    scope_id: usize,
}

impl<T: Clone + Eq + Hash + Display> Sink<T> {
    pub(crate) fn new(
        data_tx: Sender<DataFrame<ScopedKey<T>>>, control_tx: Sender<ControlMessage<ScopedKey<T>>>, clock: Clock,
        scope: String, scope_id: usize,
    ) -> Sink<T> {
        // Register our scope with the receiver.  We send this over the data channel because the
        // control channel is usually smaller, so we have a better chance of not deadlocking
        // ourselves by creating many sinks before the receiver is started, alsooooo, we could get
        // into a situation where a sink sent metrics that got processed before the scope was
        // registered depending on when it was creating during a single turn of the receiver.
        //
        // Long story short, we need to make sure we register the scope before processing any data
        // samples, so we shove them both into the same channel to get that guarantee.
        let _ = data_tx.send(DataFrame::Scope(scope_id, scope.clone()));

        Sink {
            data_tx,
            control_tx,
            clock,
            scope,
            scope_id,
        }
    }

    /// Creates a scoped clone of this `Sink`.
    ///
    /// Scoping controls the resulting metric name for any metrics sent by this `Sink`.  For
    /// example, with an imaginary `MetricType::MessagesSent`, you may normally display that
    /// as `messages_sent`.
    ///
    /// With scoping, you could have independent versions of the same metric.  This is useful for
    /// having the same "base" metric name but with broken down values.
    ///
    /// Going further with the above example, if you had a server, and listened on multiple
    /// addresses, maybe you would have a scoped `Sink` per listener, and could end up with metrics
    /// that look like this:
    /// - `listener.a.messages_sent`
    /// - `listener.b.messages_sent`
    /// - `listener.c.messages_sent`
    /// - etc
    ///
    /// Scopes are also inherited.  If you create a scoped `Sink` from another `Sink` which is
    /// already scoped, the scopes will be merged together using a `.` as the string separator.
    /// This makes it easy to nest scopes.  These scopes are fully contained, though, and so a
    /// scoped child `Sink` does not depend on its parent to exist.
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
            self.data_tx.clone(),
            self.control_tx.clone(),
            self.clock.clone(),
            new_scope,
            get_scope_id(),
        ))
    }

    /// Reference to the internal high-speed clock interface.
    pub fn clock(&self) -> &Clock { &self.clock }

    /// Sends a metric sample to the receiver.
    pub fn send(&mut self, sample: Sample<T>) -> Result<(), io::Error> {
        self.data_tx
            .send(DataFrame::Sample(sample.into_scoped(self.scope_id)))
            .map_err(|_| io_error("failed to send sample"))
    }

    /// Registers a facet with the receiver.
    pub fn add_facet(&mut self, facet: Facet<T>) {
        let scoped_facet = facet.into_scoped(self.scope_id);
        let _ = self.control_tx.send(ControlMessage::AddFacet(scoped_facet));
    }

    /// Deregisters a facet from the receiver.
    pub fn remove_facet(&mut self, facet: Facet<T>) {
        let scoped_facet = facet.into_scoped(self.scope_id);
        let _ = self.control_tx.send(ControlMessage::RemoveFacet(scoped_facet));
    }
}

impl<T: Clone + Eq + Hash + Display> Clone for Sink<T> {
    fn clone(&self) -> Sink<T> {
        Sink {
            data_tx: self.data_tx.clone(),
            control_tx: self.control_tx.clone(),
            clock: self.clock.clone(),
            scope: self.scope.clone(),
            scope_id: self.scope_id,
        }
    }
}
