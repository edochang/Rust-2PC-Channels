//!
//! participant.rs
//! Implementation of 2PC participant
//!
extern crate ipc_channel;
extern crate log;
extern crate rand;
extern crate stderrlog;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::thread;

use participant::rand::prelude::*;
use participant::ipc_channel::ipc::IpcReceiver as Receiver;
use participant::ipc_channel::ipc::TryRecvError;
use participant::ipc_channel::ipc::IpcSender as Sender;

use message::MessageType;
use message::ProtocolMessage;
use message::RequestStatus;
use oplog;

///
/// ParticipantState
/// enum for Participant 2PC state machine
///
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticipantState {
    Quiescent,
    ReceivedP1,
    VotedAbort,
    VotedCommit,
    AwaitingGlobalDecision,
}

///
/// Participant
/// Structure for maintaining per-participant state and communication/synchronization objects to/from coordinator
///
#[derive(Debug)]
pub struct Participant {
    id_str: String,
    state: ParticipantState,
    log: oplog::OpLog,
    running: Arc<AtomicBool>,
    send_success_prob: f64,
    operation_success_prob: f64,
    successful_ops: u64,
    failed_ops: u64,
    unknown_ops: u64,
    coord_tx: Sender<ProtocolMessage>,
    child_rx: Receiver<ProtocolMessage>,
    total_num_requests: u32,

}

///
/// Participant
/// Implementation of participant for the 2PC protocol
/// Required:
/// 1. new -- Constructor
/// 2. pub fn report_status -- Reports number of committed/aborted/unknown for each participant
/// 3. pub fn protocol() -- Implements participant side protocol for 2PC
///
impl Participant {

    ///
    /// new()
    ///
    /// Return a new participant, ready to run the 2PC protocol with the coordinator.
    ///
    /// HINT: You may want to pass some channels or other communication
    ///       objects that enable coordinator->participant and participant->coordinator
    ///       messaging to this constructor.
    /// HINT: You may want to pass some global flags that indicate whether
    ///       the protocol is still running to this constructor. There are other
    ///       ways to communicate this, of course.
    ///
    pub fn new(
        id_str: String,
        log_path: String,
        r: Arc<AtomicBool>,
        send_success_prob: f64,
        operation_success_prob: f64,
        coord_tx: Sender<ProtocolMessage>,
        child_rx: Receiver<ProtocolMessage>,
        total_num_requests: u32) -> Participant {

        Participant {
            id_str: format!("participant_{}", id_str),
            state: ParticipantState::Quiescent,
            log: oplog::OpLog::new(log_path),
            running: r,
            send_success_prob: send_success_prob,
            operation_success_prob: operation_success_prob,
            // TODO
            coord_tx,
            child_rx,
            successful_ops: 0,
            failed_ops: 0,
            unknown_ops: 0,
            total_num_requests,
        }
    }

    ///
    /// send()
    /// Send a protocol message to the coordinator. This can fail depending on
    /// the success probability. For testing purposes, make sure to not specify
    /// the -S flag so the default value of 1 is used for failproof sending.
    ///
    /// HINT: You will need to implement the actual sending
    ///
    pub fn send(&mut self, pm: ProtocolMessage) {
        let x: f64 = random();
        if x <= self.send_success_prob {
            // TODO: Send success
            match self.coord_tx.send(pm) {
                Ok(()) => (),
                Err(error) => {
                    error!("{}::Could not send to coordinator", self.id_str);
                    panic!("{}::error: {:#?}", self.id_str, error);
                },
            }
        } else {
            // TODO: Send fail
            error!("{}::Fail to send to coordinator", self.id_str)
        }
    }

    ///
    /// perform_operation
    /// Perform the operation specified in the 2PC proposal,
    /// with some probability of success/failure determined by the
    /// command-line option success_probability.
    ///
    /// HINT: The code provided here is not complete--it provides some
    ///       tracing infrastructure and the probability logic.
    ///       Your implementation need not preserve the method signature
    ///       (it's ok to add parameters or return something other than
    ///       bool if it's more convenient for your design).
    ///
    pub fn perform_operation(&mut self, request_option: &Option<ProtocolMessage>) -> bool {

        trace!("{}::Performing operation", self.id_str.clone());
        let x: f64 = random();
        if x <= self.operation_success_prob {
            // TODO: Successful operation
            //info!("{}::x({}), operation_success_prob({})", self.id_str, x, self.operation_success_prob);
            let mut pm_log = request_option.clone().unwrap();
            pm_log.mtype = MessageType::ParticipantVoteCommit;
            self.log.append(pm_log.mtype,pm_log.txid, pm_log.senderid, pm_log.opid);
        } else {
            // TODO: Failed operation
            let mut pm_log = request_option.clone().unwrap();
            pm_log.mtype = MessageType::ParticipantVoteAbort;
            self.log.append(pm_log.mtype,pm_log.txid, pm_log.senderid, pm_log.opid);

            return false;
        }

        true
    }

    ///
    /// report_status()
    /// Report the abort/commit/unknown status (aggregate) of all transaction
    /// requests made by this coordinator before exiting.
    ///
    pub fn report_status(&mut self) {
        // TODO: Collect actual stats
        let successful_ops: u64 = self.successful_ops;
        let failed_ops: u64 = self.failed_ops;
        let unknown_ops: u64 = self.unknown_ops;

        println!("{:16}:\t{}: {:6}\tAborted: {:6}\tUnknown: {:6}", self.id_str.clone(), format!("{:?}",RequestStatus::Committed), successful_ops, failed_ops, unknown_ops);
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
                        let pm = ProtocolMessage::instantiate(MessageType::ClientRequest, 0, String::from(""), String::from(""), 0);
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
    /// protocol()
    /// Implements the participant side of the 2PC protocol
    /// HINT: If the simulation ends early, don't keep handling requests!
    /// HINT: Wait for some kind of exit signal before returning from the protocol!
    ///
    pub fn protocol(&mut self) {
        trace!("{}::Beginning protocol", self.id_str.clone());

        // TODO
        let mut num_requests = 0;

        while self.running.load(Ordering::Relaxed) && num_requests < self.total_num_requests {
            // Receive proposal from coordinator
            let mut request_option_p1 = match self.child_rx.recv() {
                Ok(message) => message,
                Err(error) => {
                    error!("{}::Could not receive from coordinator", self.id_str);
                    panic!("{}::error: {:#?}", self.id_str, error);
                }
            };
            self.state = ParticipantState::ReceivedP1;

            if self.perform_operation(&Some(request_option_p1.clone())) {
                request_option_p1.mtype = MessageType::ParticipantVoteCommit;
                self.state = ParticipantState::VotedCommit;
            } else {
                request_option_p1.mtype = MessageType::ParticipantVoteAbort;
                self.state = ParticipantState::VotedAbort;
            }

            let vote_pm = request_option_p1.clone();
            self.send(vote_pm);

            self.state = ParticipantState::AwaitingGlobalDecision;

            let request_option_p2 = match self.child_rx.recv() {
                Ok(message) => Some(message),
                Err(error) => {
                    error!("{}::Could not receive from coordinator", self.id_str);
                    panic!("{}::error: {:#?}", self.id_str, error);
                }
            };

            let pm = request_option_p2.clone().unwrap();
            let pm_log = request_option_p2.clone().unwrap();
            if pm.mtype == MessageType::CoordinatorCommit {
                self.successful_ops += 1;
                self.log.append(pm_log.mtype,pm_log.txid, pm_log.senderid, pm_log.opid);
            } else {
                if pm.mtype == MessageType::CoordinatorAbort {
                    if pm.txid.contains("unknown") {
                        self.unknown_ops += 1;
                    } else {
                        self.failed_ops += 1;
                        self.log.append(pm_log.mtype,pm_log.txid, pm_log.senderid, pm_log.opid);
                    }
                }
            }

            self.state = ParticipantState::Quiescent;

            num_requests += 1;
        }

        self.wait_for_exit_signal();
        self.report_status();
    }
}
