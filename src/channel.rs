use lazycell::{AtomicLazyCell, LazyCell};
use mio::{Evented, Poll, PollOpt, Ready, Registration, SetReadiness, Token};
use crossbeam_channel;
use std::any::Any;
use std::error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::{fmt, io};

pub fn channel<T>(bound: usize) -> (Sender<T>, Receiver<T>) {
    let (tx_ctl, rx_ctl) = control();
    let (tx, rx) = crossbeam_channel::bounded(bound);

    let tx = Sender { tx, ctl: tx_ctl };
    let rx = Receiver { rx, ctl: rx_ctl };

    (tx, rx)
}

fn control() -> (SenderControl, ReceiverControl) {
    let inner = Arc::new(Inner {
        pending: AtomicUsize::new(0),
        senders: AtomicUsize::new(1),
        set_readiness: AtomicLazyCell::new(),
    });

    let tx = SenderControl { inner: Arc::clone(&inner) };

    let rx = ReceiverControl {
        registration: LazyCell::new(),
        inner,
    };

    (tx, rx)
}

struct SenderControl {
    inner: Arc<Inner>,
}

struct ReceiverControl {
    registration: LazyCell<Registration>,
    inner: Arc<Inner>,
}

pub struct Sender<T> {
    tx: crossbeam_channel::Sender<T>,
    ctl: SenderControl,
}

pub struct Receiver<T> {
    rx: crossbeam_channel::Receiver<T>,
    ctl: ReceiverControl,
}

pub enum SendError<T> {
    Io(io::Error),
    Full(T),
    Disconnected(T),
}

pub enum RecvError {
    Empty,
    Disconnected,
}

struct Inner {
    pending: AtomicUsize,
    senders: AtomicUsize,
    set_readiness: AtomicLazyCell<SetReadiness>,
}

impl<T> Sender<T> {
    pub fn send(&self, t: T) -> Result<(), SendError<T>> {
        match self.tx.is_full() {
            true => Err(SendError::Full(t)),
            false => {
                self.tx.send(t);
                let _ = self.ctl.mark_send();
                Ok(())
            }
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Sender<T> {
        Sender {
            tx: self.tx.clone(),
            ctl: self.ctl.clone(),
        }
    }
}

impl<T> Receiver<T> {
    pub fn recv(&self) -> Result<T, RecvError> {
        if self.ctl.inner.senders.load(Ordering::Relaxed) == 0 {
            return Err(RecvError::Disconnected);
        }

        match self.rx.try_recv() {
            None => Err(RecvError::Empty),
            Some(msg) => {
                let _ = self.ctl.mark_receive();
                Ok(msg)
            }
        }
    }
}

impl<T> Evented for Receiver<T> {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.ctl.register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        self.ctl.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        self.ctl.deregister(poll)
    }
}

impl SenderControl {
    fn mark_send(&self) -> io::Result<()> {
        let cnt = self.inner.pending.fetch_add(1, Ordering::Acquire);
        if cnt == 0 {
            if let Some(set_readiness) = self.inner.set_readiness.borrow() {
                try!(set_readiness.set_readiness(Ready::readable()));
            }
        }

        Ok(())
    }
}

impl Clone for SenderControl {
    fn clone(&self) -> SenderControl {
        self.inner.senders.fetch_add(1, Ordering::Relaxed);
        SenderControl { inner: Arc::clone(&self.inner) }
    }
}

impl Drop for SenderControl {
    fn drop(&mut self) {
        if self.inner.senders.fetch_sub(1, Ordering::Release) == 1 {
            // Still no clue why this is necessary.
            let _ = self.mark_send();
        }
    }
}

impl ReceiverControl {
    fn mark_receive(&self) -> io::Result<()> {
        let first = self.inner.pending.load(Ordering::Acquire);
        if first == 1 {
            // We're empty now after receiving the message, so mark it.
            if let Some(set_readiness) = self.inner.set_readiness.borrow() {
                try!(set_readiness.set_readiness(Ready::empty()));
            }
        }

        let second = self.inner.pending.fetch_sub(1, Ordering::AcqRel);
        if first == 1 && second > 1 {
            // More messages came in between our last receive and now, so make sure we mark the
            // channel as readable again.
            if let Some(set_readiness) = self.inner.set_readiness.borrow() {
                try!(set_readiness.set_readiness(Ready::readable()));
            }
        }

        Ok(())
    }
}

impl Evented for ReceiverControl {
    fn register(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        if self.registration.borrow().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "receiver already registered",
            ));
        }

        let (registration, set_readiness) = Registration::new2();
        poll.register(&registration, token, interest, opts)?;

        if self.inner.pending.load(Ordering::Relaxed) > 0 {
            let _ = set_readiness.set_readiness(Ready::readable());
        }

        self.registration.fill(registration).expect(
            "unexpected state encountered",
        );
        self.inner.set_readiness.fill(set_readiness).expect(
            "unexpected state encountered",
        );

        Ok(())
    }

    fn reregister(
        &self,
        poll: &Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        match self.registration.borrow() {
            Some(registration) => poll.reregister(registration, token, interest, opts),
            None => Err(io::Error::new(
                io::ErrorKind::Other,
                "receiver not registered",
            )),
        }
    }

    fn deregister(&self, poll: &Poll) -> io::Result<()> {
        match self.registration.borrow() {
            Some(registration) => poll.deregister(registration),
            None => Err(io::Error::new(
                io::ErrorKind::Other,
                "receiver not registered",
            )),
        }
    }
}

impl<T> From<io::Error> for SendError<T> {
    fn from(src: io::Error) -> SendError<T> {
        SendError::Io(src)
    }
}

impl<T> From<mpsc::TrySendError<T>> for SendError<T> {
    fn from(src: mpsc::TrySendError<T>) -> SendError<T> {
        match src {
            mpsc::TrySendError::Full(v) => SendError::Full(v),
            mpsc::TrySendError::Disconnected(v) => SendError::Disconnected(v),
        }
    }
}

impl<T: Any> error::Error for SendError<T> {
    fn description(&self) -> &str {
        match *self {
            SendError::Io(ref io_err) => io_err.description(),
            SendError::Full(..) => "full",
            SendError::Disconnected(..) => "disconnected",
        }
    }
}

impl<T> fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_send_error(self, f)
    }
}

impl<T> fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_send_error(self, f)
    }
}

#[inline]
fn format_send_error<T>(e: &SendError<T>, f: &mut fmt::Formatter) -> fmt::Result {
    match *e {
        SendError::Io(ref io_err) => write!(f, "{}", io_err),
        SendError::Full(..) => write!(f, "full"),
        SendError::Disconnected(..) => write!(f, "disconnected"),
    }
}

impl error::Error for RecvError {
    fn description(&self) -> &str {
        match *self {
            RecvError::Empty => "empty",
            RecvError::Disconnected => "disconnected",
        }
    }
}

impl fmt::Debug for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_recv_error(self, f)
    }
}

impl fmt::Display for RecvError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        format_recv_error(self, f)
    }
}

#[inline]
fn format_recv_error(e: &RecvError, f: &mut fmt::Formatter) -> fmt::Result {
    match *e {
        RecvError::Empty => write!(f, "empty"),
        RecvError::Disconnected => write!(f, "disconnected"),
    }
}


#[cfg(test)]
mod tests {
    use super::{channel, RecvError};
    use mio::{Poll, Events, Token, PollOpt, Ready};
    use std::thread;
    use std::time::Duration;

    #[test]
    pub fn test_poll_channel_edge() {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let (tx, rx) = channel(16);

        poll.register(&rx, Token(123), Ready::readable(), PollOpt::edge()).unwrap();

        // Wait, but nothing should happen
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Push the value
        tx.send("hello").unwrap();

        // Polling will contain the event
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(1, num);

        let event = events.iter().next().unwrap();
        assert_eq!(event.token(), Token(123));
        assert_eq!(event.readiness(), Ready::readable());

        // Poll again and there should be no events
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Read the value
        assert_eq!("hello", rx.recv().unwrap());

        // Poll again, nothing
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Push a value
        tx.send("goodbye").unwrap();

        // Have an event
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(1, num);

        let event = events.iter().next().unwrap();
        assert_eq!(event.token(), Token(123));
        assert_eq!(event.readiness(), Ready::readable());

        // Read the value
        rx.recv().unwrap();

        // Drop the sender half
        drop(tx);

        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(1, num);

        let event = events.iter().next().unwrap();
        assert_eq!(event.token(), Token(123));
        assert_eq!(event.readiness(), Ready::readable());

        match rx.recv() {
            Err(RecvError::Disconnected) => {}
            no => panic!("unexpected value {:?}", no),
        }
    }

    #[test]
    pub fn test_poll_channel_oneshot() {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let (tx, rx) = channel(16);

        poll.register(&rx, Token(123), Ready::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

        // Wait, but nothing should happen
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Push the value
        tx.send("hello").unwrap();

        // Polling will contain the event
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(1, num);

        let event = events.iter().next().unwrap();
        assert_eq!(event.token(), Token(123));
        assert_eq!(event.readiness(), Ready::readable());

        // Poll again and there should be no events
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Read the value
        assert_eq!("hello", rx.recv().unwrap());

        // Poll again, nothing
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Push a value
        tx.send("goodbye").unwrap();

        // Poll again, nothing
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Reregistering will re-trigger the notification
        for _ in 0..3 {
            poll.reregister(&rx, Token(123), Ready::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

            // Have an event
            let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
            assert_eq!(1, num);

            let event = events.iter().next().unwrap();
            assert_eq!(event.token(), Token(123));
            assert_eq!(event.readiness(), Ready::readable());
        }

        // Get the value
        assert_eq!("goodbye", rx.recv().unwrap());

        poll.reregister(&rx, Token(123), Ready::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

        // Have an event
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        poll.reregister(&rx, Token(123), Ready::readable(), PollOpt::edge() | PollOpt::oneshot()).unwrap();

        // Have an event
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);
    }

    #[test]
    pub fn test_poll_channel_level() {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let (tx, rx) = channel(16);

        poll.register(&rx, Token(123), Ready::readable(), PollOpt::level()).unwrap();

        // Wait, but nothing should happen
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Push the value
        tx.send("hello").unwrap();

        // Polling will contain the event
        for i in 0..5 {
            let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
            assert!(1 == num, "actually got {} on iteration {}", num, i);

            let event = events.iter().next().unwrap();
            assert_eq!(event.token(), Token(123));
            assert_eq!(event.readiness(), Ready::readable());
        }

        // Read the value
        assert_eq!("hello", rx.recv().unwrap());

        // Wait, but nothing should happen
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);
    }

    #[test]
    pub fn test_poll_channel_writable() {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let (tx, rx) = channel(16);

        poll.register(&rx, Token(123), Ready::writable(), PollOpt::edge()).unwrap();

        // Wait, but nothing should happen
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);

        // Push the value
        tx.send("hello").unwrap();

        // Wait, but nothing should happen
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);
    }

    #[test]
    pub fn test_dropping_receive_before_poll() {
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);
        let (tx, rx) = channel(16);

        poll.register(&rx, Token(123), Ready::readable(), PollOpt::edge()).unwrap();

        // Push the value
        tx.send("hello").unwrap();

        // Drop the receive end
        drop(rx);

        // Wait, but nothing should happen
        let num = poll.poll(&mut events, Some(Duration::from_millis(300))).unwrap();
        assert_eq!(0, num);
    }

    #[test]
    pub fn test_sending_from_other_thread_while_polling() {
        const ITERATIONS: usize = 20;
        const THREADS: usize = 5;

        // Make sure to run multiple times
        let poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(1024);

        for _ in 0..ITERATIONS {
            let (tx, rx) = channel(16);
            poll.register(&rx, Token(0), Ready::readable(), PollOpt::edge()).unwrap();

            for _ in 0..THREADS {
                let tx = tx.clone();

                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(50));
                    tx.send("ping").unwrap();
                });
            }

            let mut recv = 0;

            while recv < THREADS {
                let num = poll.poll(&mut events, None).unwrap();
                if num != 0 {
                    assert_eq!(1, num);
                    assert_eq!(events.iter().next().unwrap().token(), Token(0));

                    while let Ok(_) = rx.recv() {
                        recv += 1;
                    }
                }
            }
        }
    }
}
