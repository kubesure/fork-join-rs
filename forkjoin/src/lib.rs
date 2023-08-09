use anyhow::Result;

use crossbeam::channel::Receiver;
use crossbeam_utils::sync::WaitGroup;
use std::any::Any;
use thiserror::Error;

#[derive(Debug)]
struct FJResult<'a> {
    id: String,
    x: Box<dyn Any>,
    err: &'a FJError<'a>,
}

#[derive(Debug, Error)]
#[non_exhaustive]
enum FJError<'a> {
    #[error("error code: {0} Internal server error")]
    InternalServer(&'a str),
    #[error("error code: {0} Request error")]
    RequestError(&'a str),
    #[error("error code: {0} Response error")]
    ResponseError(&'a str),
    #[error("error code: {0} Connection error")]
    ConnectionError(&'a str),
    #[error("error code: {0} Concurrency context error")]
    ConcurrencyContextError(&'a str),
}

struct Input<'a> {
    id: u32,
    x: Box<dyn Any>,
    wg: &'a WaitGroup,
}

struct Multiplexer {
    workers: Vec<Box<dyn Worker>>,
}

trait Worker {
    fn work(&self) -> Result<Receiver<FJResult>, Receiver<Heartbeat>>;
    fn active_dead_line_seconds(&self) -> Result<u32>;
}

struct Heartbeat {
    id: u16,
}

#[cfg(test)]
mod tests {
    //use super::*;

    #[test]
    fn it_works() {
        assert_eq!(4, 4);
    }
}
