use super::{
    data::{Facet, ScopedKey, Snapshot},
    helper::io_error,
};
use crossbeam_channel::{bounded, Sender};
use std::{fmt::Display, hash::Hash, io};

pub(crate) enum ControlMessage<T> {
    AddFacet(Facet<T>),
    RemoveFacet(Facet<T>),
    Snapshot(Sender<Snapshot>),
}

/// Dedicated handle for performing operations on a running `Receiver`.
///
/// The caller is able to request metric snapshots at any time without requiring mutable access to
/// the sink.  This all flows through the existing control mechanism, and so is very fast.
pub struct Controller<T: Clone + Eq + Hash + Display> {
    control_tx: Sender<ControlMessage<ScopedKey<T>>>,
}

impl<T: Clone + Eq + Hash + Display> Controller<T> {
    pub(crate) fn new(control_tx: Sender<ControlMessage<ScopedKey<T>>>) -> Controller<T> { Controller { control_tx } }

    /// Retrieves a snapshot of the current metric state.
    pub fn get_snapshot(&self) -> Result<Snapshot, io::Error> {
        let (tx, rx) = bounded(0);
        let msg = ControlMessage::Snapshot(tx);

        match self.control_tx.send(msg) {
            Ok(_) => {
                match rx.recv() {
                    Ok(result) => Ok(result),
                    Err(_) => Err(io_error("failed to receive snapshot")),
                }
            },
            Err(_) => Err(io_error("failed to send snapshot command")),
        }
    }
}
