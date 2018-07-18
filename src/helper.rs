use std::io::{Error, ErrorKind};

pub fn io_error(reason: &str) -> Error {
    Error::new(ErrorKind::Other, reason)
}
