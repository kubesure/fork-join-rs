use anyhow::Result;
use crossbeam::{
    channel::{after, unbounded, Receiver, Sender},
    select,
};
use crossbeam_utils::sync::WaitGroup;
use log::{error, info};
use std::time::Duration;
use std::{any::Any, sync::Arc};
use thiserror::Error;

/*#[derive(Debug)]
struct FJResult<D: Data> {
    id: String,
    data: Arc<D>,
    err: FJError,
}*/

#[derive(Debug)]
struct FJResult {
    id: String,
    data: Arc<Box<dyn Any + Send + Sync>>,
    err: FJError,
}

trait Data: Send + Sync {}

#[derive(Debug, Error)]
#[non_exhaustive]
enum FJError {
    #[error("error code: {0} Internal server error")]
    InternalServer(String),
    #[error("error code: {0} Request error")]
    RequestError(String),
    #[error("error code: {0} Response error")]
    ResponseError(String),
    #[error("error code: {0} Connection error")]
    ConnectionError(String),
    #[error("error code: {0} Concurrency context error")]
    ConcurrencyContextError(String),
}

struct Multiplexer<W: Worker> {
    workers: Vec<W>,
}

#[derive(Debug)]
struct Input<W: Worker> {
    id: u32,
    wg: WaitGroup,
    worker: W,
}

trait Worker: Send {
    fn work(&self) -> Result<(Receiver<FJResult>, Receiver<Heartbeat>)>;
    fn active_dead_line_seconds(&self) -> Result<u32>;
}
#[derive(Debug)]
struct Heartbeat {
    id: u16,
}

//TODO const INTERNAL_SERVER_ERROR_CODE: String = String::from("001");

impl<W: Worker> Multiplexer<W> {
    pub fn new() -> Self {
        Multiplexer {
            workers: Vec::new(),
        }
    }

    fn add_worker(&mut self, worker: W) {
        self.workers.push(worker);
    }

    pub fn multiplex(self) -> Result<Receiver<FJResult>, FJError> {
        if self.workers.len() == 0 {
            error!("no workers added");
            return Err(FJError::InternalServer("001".to_string()));
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
                let mutiplex_result_stream = tx.clone();

                let handle = s.spawn(move |_| {
                    manage(input, mutiplex_result_stream);
                });
            }
            wg.wait();
        });

        Ok(rx)
    }
}

fn manage<W: Worker>(input: Input<W>, multiplex_result_stream: Sender<FJResult>) {
    let wg: WaitGroup = input.wg;
    let result: Result<(Receiver<FJResult>, Receiver<Heartbeat>)> =
        dispatch(input.worker, Duration::from_secs(1));

    match result {
        Ok(results) => loop {
            select! {
                recv(results.1) -> beat => {
                    if let Ok(beat) = beat {
                        info!("received {:?}",beat.id);
                        info!("received  beat {:?}", beat);
                        return
                    }
                },
                recv(results.0) -> result => {
                    if let Ok(result) = result {
                        info!("{:?}",result);
                        multiplex_result_stream.send(result);
                        drop(wg);
                        return
                    }
                },
            }
        },
        Err(_) => {
            todo!()
        }
    }
}

fn dispatch<W: Worker>(
    worker: W,
    pulse_interval: Duration,
) -> Result<(Receiver<FJResult>, Receiver<Heartbeat>)> {
    let worker_streams: Result<(Receiver<FJResult>, Receiver<Heartbeat>)> = worker.work();
    worker_streams
}

fn send_pulse(sender: &Sender<Heartbeat>, pulse_interval_sec: u64) {
    let interval = Duration::from_secs(pulse_interval_sec);

    select! {
        recv(after(interval)) -> _ => {
            _ = sender.send(Heartbeat { id: 0 });
        }
    }
}

struct TestWorker {
    active_deadline_seconds: u32,
}
impl Worker for TestWorker {
    fn work(&self) -> Result<(Receiver<FJResult>, Receiver<Heartbeat>)> {
        info!("I am Testworker");
        let (result_tx, result_rx) = unbounded::<FJResult>();
        let (heat_beat_tx, heart_beat_rx) = unbounded::<Heartbeat>();

        let result = crossbeam::scope(|s| {
            let handle = s.spawn(|_| {
                send_pulse(&heat_beat_tx, 1);
            });
        });

        let result = crossbeam::scope(|s| {
            let self_clone = self.clone();
            let handle = s.spawn(move |sc| {
                self_clone.do_work(heat_beat_tx.clone());
            });
        });

        Ok((result_rx, heart_beat_rx))
    }

    fn active_dead_line_seconds(&self) -> Result<u32> {
        Ok(self.active_deadline_seconds)
    }
}

impl TestWorker {
    fn do_work(&self, heart_beat_tx: Sender<Heartbeat>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_worker() {
        let mut multiplexer: Multiplexer<TestWorker> = Multiplexer::new();
        multiplexer.add_worker(TestWorker {
            active_deadline_seconds: 10,
        });
        assert_eq!(multiplexer.workers.len(), 1);
        let result = multiplexer.multiplex();

        match result {
            Ok(result_stream) => loop {
                match result_stream.recv() {
                    Ok(value) => {
                        info!("Received: {}", value.id);
                    }
                    Err(_) => {
                        error!("all senders are dropped");
                        break;
                    }
                }
            },
            Err(err) => {
                error!("{}", err);
            }
        }
    }
}
