//!
//! client.rs
//! Implementation of 2PC client
//!
extern crate ipc_channel;
extern crate log;
extern crate stderrlog;

use std::{thread, error};
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;

use client::ipc_channel::ipc::IpcReceiver as Receiver;
use client::ipc_channel::ipc::TryRecvError;
use client::ipc_channel::ipc::IpcSender as Sender;

use message;
use message::MessageType;
use message::RequestStatus;

// Client state and primitives for communicating with the coordinator
#[derive(Debug)]
pub struct Client {
    pub id_str: String,
    pub running: Arc<AtomicBool>,
    pub num_requests: u32,
    // TODO
    pub coord_tx: Sender<message::ProtocolMessage>,
    pub child_rx: Receiver<message::ProtocolMessage>,
    pub successful_ops: u64,
    pub failed_ops: u64,
    pub unknown_ops: u64,
}

///
/// Client Implementation
/// Required:
/// 1. new -- constructor
/// 2. pub fn report_status -- Reports number of committed/aborted/unknown
/// 3. pub fn protocol(&mut self, n_requests: i32) -- Implements client side protocol
///
impl Client {

    ///
    /// new()
    ///
    /// Constructs and returns a new client, ready to run the 2PC protocol
    /// with the coordinator.
    ///
    /// HINT: You may want to pass some channels or other communication
    ///       objects that enable coordinator->client and client->coordinator
    ///       messaging to this constructor.
    /// HINT: You may want to pass some global flags that indicate whether
    ///       the protocol is still running to this constructor
    ///
    pub fn new(id_str: String,
               running: Arc<AtomicBool>, 
               coord_tx: Sender<message::ProtocolMessage>, 
               child_rx: Receiver<message::ProtocolMessage>) -> Client {
        Client {
            id_str: format!{"client_{}",id_str},
            num_requests: 0,
            running,
            // TODO
            coord_tx,
            child_rx,
            successful_ops: 0,
            failed_ops: 0,
            unknown_ops: 0,
        }
    }

    ///
    /// wait_for_exit_signal(&mut self)
    /// Wait until the running flag is set by the CTRL-C handler
    ///
    pub fn wait_for_exit_signal(&mut self) {
        trace!("{}::Waiting for exit signal", self.id_str.clone());

        // TODO
        while self.running.load(Ordering::Relaxed) {
            let result = match self.child_rx.try_recv() {
                Ok(protocol_message) => protocol_message,
                Err(error) => match error {
                    TryRecvError::Empty => {
                        warn!("{}::Could not receive from coordinator, error: {:#?}", self.id_str, error);
                        let pm = message::ProtocolMessage::instantiate(MessageType::ClientRequest, 0, String::from(""), String::from(""), 0);
                        pm
                    },
                    TryRecvError::IpcError(ipc_channel::ipc::IpcError::Disconnected) => {
                        warn!("{}::Could not receive from coordinator, error: {:#?}", self.id_str, error);
                        let pm = message::ProtocolMessage::instantiate(MessageType::ClientRequest, 0, String::from(""), String::from(""), 0);
                        pm
                    },
                    TryRecvError::IpcError(_) => {
                        error!("{}::Could not receive from coordinator, error: {:#?}", self.id_str, error);
                        panic!("{:#?}", error);
                    },                    
                },
            };    
    
            if result.mtype == MessageType::CoordinatorExit {
                break;
            }

            let millis = Duration::from_millis(1000);
            thread::sleep(millis);
        }

        trace!("{}::Exiting", self.id_str.clone());
    }

    ///
    /// send_next_operation(&mut self)
    /// Send the next operation to the coordinator
    ///
    pub fn send_next_operation(&mut self) {

        // Create a new request with a unique TXID.
        self.num_requests = self.num_requests + 1;
        let txid = format!("{}_op_{}", self.id_str.clone(), self.num_requests);
        let pm = message::ProtocolMessage::generate(message::MessageType::ClientRequest,
                                                    txid.clone(),
                                                    self.id_str.clone(),
                                                    self.num_requests);
        info!("{}::Sending operation #{}", self.id_str.clone(), self.num_requests);

        // TODO
        // txid = <id_str>_op_<requst#> --> 0_op_0
        match self.coord_tx.send(pm) {
            Ok(()) => (),
            Err(error) => {
                error!("{}::Could not send to coordinator", self.id_str);
                panic!("{}::error: {:#?}", self.id_str, error);
            },
        };

        self.unknown_ops += 1;
        trace!("{}::Sent operation #{}", self.id_str.clone(), self.num_requests);

        
    }

    ///
    /// recv_result()
    /// Wait for the coordinator to respond with the result for the
    /// last issued request. Note that we assume the coordinator does
    /// not fail in this simulation
    ///
    pub fn recv_result(&mut self) {

        info!("{}::Receiving Coordinator Result", self.id_str.clone());

        // TODO
        let result = match self.child_rx.recv() {
            Ok(protocol_message) => protocol_message,
            Err(error) => {
                //self.report_status();
                error!("{}::Could not receive from coordinator", self.id_str);
                panic!("{}::error: {:#?}", self.id_str, error);
            },
        };        
        
        if result.mtype == message::MessageType::ClientResultCommit {
            self.successful_ops += 1;
            self.unknown_ops -= 1;
        }

        if result.mtype == message::MessageType::ClientResultAbort {
            if !result.txid.contains("unknown"){
                self.failed_ops += 1;
                self.unknown_ops -= 1;
            }
        }

        info!("{}::Received Coordinator Response: {:?}", self.id_str.clone(), result);
    }

    ///
    /// report_status()
    /// Report the abort/commit/unknown status (aggregate) of all transaction
    /// requests made by this client before exiting.
    ///
    pub fn report_status(&mut self) {
        // TODO: Collect actual stats
        let successful_ops: u64 = self.successful_ops;
        let failed_ops: u64 = self.failed_ops;
        let unknown_ops: u64 = self.unknown_ops;

        println!("{:16}:\t{}: {:6}\tAborted: {:6}\tUnknown: {:6}", self.id_str.clone(), format!("{:?}",RequestStatus::Committed), successful_ops, failed_ops, unknown_ops);
    }

    ///
    /// protocol()
    /// Implements the client side of the 2PC protocol
    /// HINT: if the simulation ends early, don't keep issuing requests!
    /// HINT: if you've issued all your requests, wait for some kind of
    ///       exit signal before returning from the protocol method!
    ///
    pub fn protocol(&mut self, n_requests: u32) {

        // TODO
        debug!("{}::protocol(): Enter Client Protocol", self.id_str);

        for _ in 0..n_requests {
            if self.running.load(Ordering::Relaxed) {
                self.send_next_operation();
                self.recv_result();
            } else {
                break;
            }
        }

        self.wait_for_exit_signal();
        self.report_status();
    }
}
