use super::{data::Snapshot, helper::io_error};
use crossbeam_channel::{bounded, Sender};
use std::io;

/// Various control actions performed by a controller.
pub(crate) enum ControlFrame {
    /// Takes a snapshot of the current metric state.
    ///
    /// Caller must supply a [`Sender`] instance which the result can be sent to.
    Snapshot(Sender<Snapshot>),
}

/// Dedicated handle for performing operations on a running [`Receiver`](crate::receiver::Receiver).
///
/// The caller is able to request metric snapshots at any time without requiring mutable access to
/// the sink.  This all flows through the existing control mechanism, and so is very fast.
#[derive(Clone)]
pub struct Controller {
    control_tx: Sender<ControlFrame>,
}

impl Controller {
    pub(crate) fn new(control_tx: Sender<ControlFrame>) -> Controller { Controller { control_tx } }

    /// Retrieves a snapshot of the current metric state.
    pub fn get_snapshot(&self) -> Result<Snapshot, io::Error> {
        let (tx, rx) = bounded(0);
        let msg = ControlFrame::Snapshot(tx);

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
