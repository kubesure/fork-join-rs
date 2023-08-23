use anyhow::Result;
use crossbeam::{
    channel::{after, unbounded, Receiver, Sender},
    select,
};
use log::{error, info};
use std::{any::Any, sync::Arc};
use std::{rc::Rc, time::Duration};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct FJResult {
    pub id: String,
    pub data: Arc<Box<dyn Any + Send + Sync>>,
    err: FJError,
}

trait Data: Send + Sync {}

#[derive(Debug, Error, Clone)]
#[non_exhaustive]
pub enum FJError {
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

//TODO make works accessible by a method access
pub struct Multiplexer<W: Worker> {
    workers: Vec<W>,
    channels: (Sender<FJResult>, Receiver<FJResult>),
}

#[derive(Debug)]
struct Input<'a, W: Worker> {
    id: u32,
    worker: Arc<&'a W>,
}

impl<'a, W: Worker> Clone for Input<'a, W> {
    fn clone(&self) -> Self {
        Input {
            id: self.id,
            worker: self.worker.clone(),
        }
    }
}

pub trait Worker: Send {
    fn work(&self) -> (Receiver<FJResult>, Receiver<Heartbeat>);
    fn active_dead_line_seconds(&self) -> Result<u32>;

    //TODO try to make this function work in threads
    /*
    fn send_pulse(&self, sender: Sender<Heartbeat>, mut pulse_interval_sec: u64) {
        let interval = Duration::from_secs(pulse_interval_sec);

        loop {
            select! {
                recv(after(interval)) -> _ => {
                    _ = sender.send(Heartbeat { id: 0 });
                    info!("heart {} beat sent", pulse_interval_sec);
                    pulse_interval_sec += 1;
                }
            }
        }
    }*/
}
#[derive(Debug)]
pub struct Heartbeat {
    id: u16,
}

fn send_pulse(sender: Sender<Heartbeat>, mut pulse_interval_sec: u64) {
    let interval = Duration::from_secs(pulse_interval_sec);

    loop {
        select! {
            recv(after(interval)) -> _ => {
                _ = sender.send(Heartbeat { id: 0 });
                info!("heart {} beat sent", pulse_interval_sec);
                pulse_interval_sec += 1;
            }
        }
    }
}

//TODO const INTERNAL_SERVER_ERROR_CODE: String = String::from("001");

impl<W: Worker + std::marker::Sync> Multiplexer<W> {
    pub fn new() -> Self {
        Multiplexer {
            workers: Vec::new(),
            channels: unbounded(),
        }
    }

    pub fn add_worker(&mut self, worker: W) {
        self.workers.push(worker);
    }

    pub fn multiplex(&self) -> Result<(), FJError> {
        if self.workers.len() == 0 {
            error!("no workers added");
            return Err(FJError::InternalServer("001".to_string()));
        }

        _ = crossbeam::scope(|s| {
            for (index, wrkr) in self.workers.iter().enumerate() {
                let input = Input {
                    id: index as u32 + 1,
                    worker: wrkr.into(),
                };
                let sender_clone = self.channels.0.clone();
                _ = s.spawn(move |_| {
                    manage(input, sender_clone);
                });
            }
        });
        Ok(())
    }

    fn get_receiver(&self) -> &Receiver<FJResult> {
        &self.channels.1
    }
}

fn manage<W: Worker>(input: Input<W>, multiplex_result_stream: Sender<FJResult>) {
    let results = input.worker.work();
    let mut received_result = false;

    loop {
        select! {
            recv(results.1) -> beat => {
                if let Ok(beat) = beat {
                    info!("received  beat {:?}", beat);
                }
            },
            recv(results.0) -> result => {
                if let Ok(result) = result {
                    info!("{:?}",result);
                    //TODO handle result
                    _ = multiplex_result_stream.send(result);
                    received_result = true;
                }
            },
        }
        if received_result {
            drop(results.1);
            drop(results.0);
            break;
        }
    }
}

#[cfg(test)]
pub mod tests {
    use anyhow::Ok;

    use super::*;

    pub struct TestWorker {
        pub active_deadline_seconds: u32,
    }

    impl Worker for TestWorker {
        fn work(&self) -> (Receiver<FJResult>, Receiver<Heartbeat>) {
            let (result_tx, result_rx) = unbounded::<FJResult>();
            let (heat_beat_tx, heart_beat_rx) = unbounded::<Heartbeat>();

            _ = std::thread::spawn(|| {
                send_pulse(heat_beat_tx, 1);
            });

            _ = std::thread::spawn(|| {
                do_work(result_tx);
            });

            (result_rx, heart_beat_rx)
        }

        fn active_dead_line_seconds(&self) -> Result<u32> {
            Ok(self.active_deadline_seconds)
        }
    }

    fn do_work(result_tx: Sender<FJResult>) {
        let result = FJResult {
            id: "001".to_string(),
            data: Arc::new(Box::new(42)),
            err: FJError::InternalServer("001".to_string()),
        };
        std::thread::sleep(Duration::from_secs(5));
        match result_tx.send(result) {
            std::result::Result::Ok(_) => {}
            Err(err) => {
                error!("Error while sending result {}", err.to_string())
            }
        }
    }

    #[test]
    pub fn test_add_worker() {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .format_timestamp(None)
            .format_module_path(false)
            .init();

        let mut multiplexer: Multiplexer<TestWorker> = Multiplexer::new();

        multiplexer.add_worker(TestWorker {
            active_deadline_seconds: 10,
        });
        assert_eq!(multiplexer.workers.len(), 1);

        let result = multiplexer.multiplex();

        let receiver = multiplexer.get_receiver();

        loop {
            match receiver.recv() {
                std::result::Result::Ok(value) => {
                    info!("Received: {}", value.id);

                    if let Some(data) = value.data.downcast_ref::<i32>() {
                        info!("hey received {}", *data);
                        assert_eq!(*data, 42);
                    } else {
                        panic!("should have go some value");
                    }
                    break;
                }
                Err(_) => {
                    error!("all workers are dropped");
                    break;
                }
            }
        }
    }
}
