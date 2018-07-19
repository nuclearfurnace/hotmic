use std::io;
use std::sync::mpsc;
use helper::io_error;
use channel::{Sender, SendError};
use data::{Facet, Snapshot};

pub(crate) enum ControlMessage<T> {
    AddFacet(Facet<T>),
    RemoveFacet(Facet<T>),
    Snapshot(mpsc::SyncSender<Snapshot<T>>),
}

/// Dedicated handle for performing operations on a running `Receiver`.
///
/// The caller is able to request metric snapshots at any time without requiring mutable access to
/// the sink.  This all flows through the existing control mechanism, and so is very fast.
pub struct Controller<T> {
    control_tx: Sender<ControlMessage<T>>,
}

impl<T> Controller<T> {
    pub(crate) fn new(control_tx: Sender<ControlMessage<T>>) -> Controller<T> {
        Controller { control_tx: control_tx }
    }

    /// Retrieves a snapshot of the current metric state.
    pub fn get_snapshot(&self) -> Result<Snapshot<T>, io::Error> {
        let (tx, rx) = mpsc::sync_channel(1);
        let msg = ControlMessage::Snapshot(tx);

        match self.control_tx.send(msg) {
            Ok(_) => match rx.recv() {
                Ok(result) => Ok(result),
                Err(_) => Err(io_error("failed to receive snapshot")),
            },
            Err(e) => match e {
                SendError::Io(e) => Err(e),
                SendError::Full(_) | SendError::Disconnected(_) => Err(io_error("failed to send snapshot command")),
            },
        }
    }
}
