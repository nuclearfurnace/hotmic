use std::io;
use std::hash::Hash;
use channel;
use control::ControlMessage;
use data::{Facet, Sample};
use helper::io_error;
use crossbeam_channel::Receiver;

/// An independent handle for sending metric samples into the receiver.
///
/// `Sink` is cloneable, and can not only send metric samples but can register and deregister
/// metric facets at any time.
pub struct Sink<T> {
    buffer_pool_rx: Receiver<Vec<Sample<T>>>,
    data_tx: channel::Sender<Vec<Sample<T>>>,
    control_tx: channel::Sender<ControlMessage<T>>,
    buffer: Option<Vec<Sample<T>>>,
    batch_size: usize,
}

impl<T> Sink<T>
    where T: Eq + Hash
{
    pub(crate) fn new(
        buffer_pool_rx: Receiver<Vec<Sample<T>>>,
        data_tx: channel::Sender<Vec<Sample<T>>>,
        control_tx: channel::Sender<ControlMessage<T>>,
        batch_size: usize,
    ) -> Sink<T> {
        Sink {
            buffer_pool_rx: buffer_pool_rx,
            data_tx: data_tx,
            control_tx: control_tx,
            buffer: None,
            batch_size: batch_size,
        }
    }

    /// Sends a metric sample to the receiver.
    pub fn send(&mut self, sample: Sample<T>) -> Result<(), io::Error> {
        let mut buffer = match self.buffer.take() {
            None => {
                self.buffer_pool_rx.recv()
                    .ok_or(io_error("failed to get sample buffer"))?
            },
            Some(buffer) => buffer,
        };

        buffer.push(sample);
        if buffer.len() >= self.batch_size {
            self.data_tx.send(buffer)
                .map_err(|_| io_error("failed to send sample buffer"))?;
        } else {
            self.buffer = Some(buffer);
        }

        Ok(())
    }

    /// Registers a facet with the receiver.
    pub fn add_facet(&mut self, facet: Facet<T>) {
        let _ = self.control_tx.send(ControlMessage::AddFacet(facet));
    }

    /// Deregisters a facet from the receiver.
    pub fn remove_facet(&mut self, facet: Facet<T>) {
        let _ = self.control_tx.send(ControlMessage::RemoveFacet(facet));
    }
}
