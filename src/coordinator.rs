//!
//! coordinator.rs
//! Implementation of 2PC coordinator
//!
extern crate log;
extern crate stderrlog;
extern crate rand;
extern crate ipc_channel;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use coordinator::ipc_channel::ipc::IpcSender as Sender;
use coordinator::ipc_channel::ipc::IpcReceiver as Receiver;
use coordinator::ipc_channel::ipc::TryRecvError;
use coordinator::ipc_channel::ipc::channel;

use message;
use message::MessageType;
use message::ProtocolMessage;
use message::RequestStatus;
use oplog;

/// CoordinatorState
/// States for 2PC state machine
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoordinatorState {
    Quiescent,  // in a state or period of inactivity or dormancy.
    ReceivedRequest,
    ProposalSent,
    ReceivedVotesAbort,
    ReceivedVotesCommit,
    SentGlobalDecision
}

/// Coordinator
/// Struct maintaining state for coordinator
#[derive(Debug)]
pub struct Coordinator {
    state: CoordinatorState,
    running: Arc<AtomicBool>,
    log: oplog::OpLog,
    // TODO
    successful_ops: u64,
    failed_ops: u64,
    unknown_ops: u64,
    clients: HashMap<String, (Sender<ProtocolMessage>, Receiver<ProtocolMessage>)>,
    participants: HashMap<String, (Sender<ProtocolMessage>, Receiver<ProtocolMessage>)>,
    total_num_requests: u32,
}

///
/// Coordinator
/// Implementation of coordinator functionality
/// Required:
/// 1. new -- Constructor
/// 2. protocol -- Implementation of coordinator side of protocol
/// 3. report_status -- Report of aggregate commit/abort/unknown stats on exit.
/// 4. participant_join -- What to do when a participant joins
/// 5. client_join -- What to do when a client joins
///
impl Coordinator {

    ///
    /// new()
    /// Initialize a new coordinator
    ///
    /// <params>
    ///     log_path: directory for log files --> create a new log there.
    ///     r: atomic bool --> still running?
    ///
    pub fn new(
        log_path: String,
        r: &Arc<AtomicBool>, 
        total_num_requests: u32) -> Coordinator {

        Coordinator {
            state: CoordinatorState::Quiescent,
            log: oplog::OpLog::new(log_path),
            running: r.clone(),
            // TODO
            successful_ops: 0,
            failed_ops: 0,
            unknown_ops: 0,
            clients: HashMap::new(),
            participants: HashMap::new(),
            total_num_requests,
        }
    }

    ///
    /// participant_join()
    /// Adds a new participant for the coordinator to keep track of
    ///
    /// HINT: Keep track of any channels involved!
    /// HINT: You may need to change the signature of this function
    ///
    pub fn participant_join(&mut self, name: &String, child_tx: Sender<ProtocolMessage>, rx: Receiver<ProtocolMessage>) {
        assert!(self.state == CoordinatorState::Quiescent);

        // TODO
        let key = name.clone();
        self.participants.insert(key, (child_tx, rx));
    }

    ///
    /// client_join()
    /// Adds a new client for the coordinator to keep track of
    ///
    /// HINT: Keep track of any channels involved!
    /// HINT: You may need to change the signature of this function
    ///
    pub fn client_join(&mut self, name: &String, child_tx: Sender<ProtocolMessage>, rx: Receiver<ProtocolMessage>) {
        assert!(self.state == CoordinatorState::Quiescent);

        // TODO
        let key = name.clone();
        self.clients.insert(key, (child_tx, rx));
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

        println!("coordinator     :\tCommitted: {:6}\tAborted: {:6}\tUnknown: {:6}", successful_ops, failed_ops, unknown_ops);
    }

    ///
    /// protocol()
    /// Implements the coordinator side of the 2PC protocol
    /// HINT: If the simulation ends early, don't keep handling requests!
    /// HINT: Wait for some kind of exit signal before returning from the protocol!
    ///
    pub fn protocol(&mut self) {

        // TODO
        let mut num_requests = 0;

        while self.running.load(Ordering::Relaxed) && num_requests <= self.total_num_requests {
            if num_requests == self.total_num_requests {
                let pm_shutdown = message::ProtocolMessage::instantiate(MessageType::CoordinatorExit, 0, String::from(""), String::from(""), 0);
                self.send_client_message_exit(pm_shutdown.clone());
                self.send_participant_message(pm_shutdown);
                num_requests += 1;
            } 
            
            if num_requests < self.total_num_requests {
                for (_c_key, c_value) in self.clients.iter() {
                    // Receive request from client
                    let mut client_request = match c_value.1.recv() {
                        Ok(message) => message,
                        Err(error) => {
                            error!("coord::Could not receive from client");
                            panic!("coord::error: {:#?}", error);
                        },
                    };

                    self.unknown_ops += 1;

                    self.state = CoordinatorState::ReceivedRequest;

                    client_request.mtype = message::MessageType::CoordinatorPropose;
                    let participant_request_p1 = client_request.clone();
                    let mut participant_request_p2 = client_request.clone();

                    self.log.append(client_request.mtype, client_request.txid, client_request.senderid, client_request.opid);

                    // Distribute request to participants - operation to commit
                    debug!("coord::Distribute requests to participant - operation to commit");
                    self.send_participant_message(participant_request_p1);

                    self.state = CoordinatorState::ProposalSent;
                    let mut vote = 0;
                    let mut unknown = false;

                    // Receive votes from participants
                    for (_, p_value) in self.participants.iter() {
                        let mut received = false;
                        let now = std::time::Instant::now();
                        let timeout = 500;

                        while self.running.load(Ordering::Relaxed) && !received {
                            if now.elapsed().as_millis() as u64 > timeout {
                                unknown = true;
                                break;
                            }

                            let p_result = p_value.1.try_recv();
                            if p_result.is_ok() {
                                received = true;
                                let p_vote: ProtocolMessage = p_result.unwrap();

                                if p_vote.mtype == MessageType::ParticipantVoteCommit {
                                    vote += 1;
                                }
                            } else {
                                if p_result.is_err() {
                                    let error = p_result.unwrap_err();
                                    match error {
                                        TryRecvError::Empty => {
                                            trace!("coord::Could not receive from participant, error: {:#?}", error);
                                        },
                                        TryRecvError::IpcError(ipc_channel::ipc::IpcError::Disconnected) => {
                                            trace!("coord::Could not receive from participant, error: {:#?}", error);
                                        },
                                        TryRecvError::IpcError(_) => {
                                            error!("coord::Could not receive from coordinator, error: {:#?}", error);
                                            error!("{:#?}", error);
                                            //panic!("{:#?}", error);
                                        },
                                    };

                                    let millis = Duration::from_millis(100);
                                    thread::sleep(millis);
                                }
                            }
                        }
                    };

                    let mut client_request_p2 = participant_request_p2.clone();

                    if vote == self.participants.len() {
                        // Notify participants to commit
                        debug!("coord::Notify participants to commit");
                        self.state = CoordinatorState::ReceivedVotesCommit;
                        participant_request_p2.mtype = MessageType::CoordinatorCommit;
                        let log_commit = participant_request_p2.clone();
                        self.log.append(log_commit.mtype, log_commit.txid, log_commit.senderid, log_commit.opid);
                        self.send_participant_message(participant_request_p2);
                        // Notify client
                        debug!("coord::Notify clients of commit");
                        client_request_p2.mtype = MessageType::ClientResultCommit;
                        self.send_client_message(client_request_p2);
                        self.successful_ops += 1;
                        self.unknown_ops -= 1;
                    } else {
                        if unknown {
                            debug!("coord::Send unknown the participants and clients");
                            participant_request_p2.mtype = MessageType::CoordinatorAbort;
                            participant_request_p2.txid = format!("{}_unknown", participant_request_p2.txid);
                            self.send_participant_message(participant_request_p2);
                            self.send_client_message(client_request_p2);
                        } else {
                            // Notify participants to abort
                            debug!("coord::Notify participants to abort");
                            self.state = CoordinatorState::ReceivedVotesAbort;
                            participant_request_p2.mtype = MessageType::CoordinatorAbort;
                            let log_abort = participant_request_p2.clone();
                            self.log.append(log_abort.mtype, log_abort.txid, log_abort.senderid, log_abort.opid);
                            self.send_participant_message(participant_request_p2);
                            // Notify client
                            debug!("coord::Notify clients of abort");
                            client_request_p2.mtype = MessageType::ClientResultAbort;
                            self.send_client_message(client_request_p2);
                            self.failed_ops += 1;
                            self.unknown_ops -= 1;
                        }
                    }

                    num_requests += 1;
                }
            }

            self.state = CoordinatorState::Quiescent;
        }

        self.report_status();
    }

    fn send_participant_message(&self, pm: ProtocolMessage) {
        for (_p_key, p_value) in self.participants.iter() {
            match p_value.0.send(pm.clone()) {
                Ok(()) => (),
                Err(error) => {
                    error!("coord::Could not send to participant");
                    error!("coord::error: {:#?}", error);
                    //panic!("coord::error: {:#?}", error);
                    break;
                },
            }
        } 
    }

    fn send_client_message(&self, pm: ProtocolMessage) {
        let client_hmap_value = self.clients.get(&pm.senderid);
        if client_hmap_value.is_some() {
            let c_value = client_hmap_value.unwrap();

            match c_value.0.send(pm.clone()) {
                Ok(()) => (),
                Err(error) => {
                    error!("coord::Could not send to client");
                    error!("coord::error: {:#?}", error);
                    //panic!("coord::error: {:#?}", error);
                },
            } 
        }
    }

    fn send_client_message_exit(&self, pm: ProtocolMessage) {
        for (c_key, _c_value) in self.clients.iter() {
            let mut pm_mod = pm.clone();
            pm_mod.senderid = String::from(c_key);
            self.send_client_message(pm_mod);
        }
    }
}