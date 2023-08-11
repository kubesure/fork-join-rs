use anyhow::Result;

use crossbeam::channel::{unbounded, Receiver, Sender};
use crossbeam_utils::sync::WaitGroup;
use log::{error, info};
use thiserror::Error;

#[derive(Debug)]
struct FJResult<'a> {
    id: String,
    //x: Box<dyn Any>,
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

struct Multiplexer<W: Worker> {
    workers: Vec<W>,
}

struct Input<W: Worker> {
    id: u32,
    wg: WaitGroup,
    worker: W,
}

trait Worker: Send {
    fn work(&self) -> Result<Receiver<FJResult>, Receiver<Heartbeat>>;
    fn active_dead_line_seconds(&self) -> Result<u32>;
}

struct Heartbeat {
    id: u16,
}

const INTERNAL_SERVER_ERROR_CODE: &str = "001";

impl<W: Worker> Multiplexer<W> {
    pub fn new() -> Self {
        Multiplexer {
            workers: Vec::new(),
        }
    }

    fn add_worker(&mut self, worker: W) {
        self.workers.push(worker);
    }

    pub fn multiplex<'a>(self) -> Result<Receiver<FJResult<'a>>, FJError<'a>> {
        if self.workers.len() == 0 {
            error!("no workers added");
            return Err(FJError::InternalServer(INTERNAL_SERVER_ERROR_CODE));
        }

        let (tx, rx) = unbounded();

        let result = crossbeam::scope(|s| {
            let wg = WaitGroup::new();
            for (index, wrkr) in self.workers.into_iter().enumerate() {
                let wg = wg.clone();
                let input = Input {
                    id: index as u32 + 1,
                    wg: wg,
                    worker: wrkr,
                };
                let tx = tx.clone();
                let handle = s.spawn(move |_| {
                    manage(input, tx);
                });
                handle.join().unwrap();
            }
            wg.wait();
        });

        Ok(rx)
    }
}

fn manage<W: Worker>(input: Input<W>, tx: Sender<FJResult>) {
    input.worker.work();
}

struct TestWorker {}
impl Worker for TestWorker {
    fn work(&self) -> Result<Receiver<FJResult>, Receiver<Heartbeat>> {
        info!("I am Testworker");
        todo!()
    }

    fn active_dead_line_seconds(&self) -> Result<u32> {
        Ok(10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_worker() {
        let mut multiplexer: Multiplexer<TestWorker> = Multiplexer::new();
        multiplexer.add_worker(TestWorker {});
        assert_eq!(multiplexer.workers.len(), 1);
        multiplexer.multiplex();
    }
}
