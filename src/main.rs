#[macro_use]
extern crate log;
extern crate stderrlog;
extern crate clap;
extern crate ctrlc;
extern crate ipc_channel;
use std::env;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::{Child,Command};
use ipc_channel::ipc::IpcSender as Sender;
use ipc_channel::ipc::IpcReceiver as Receiver;
use ipc_channel::ipc::IpcOneShotServer;
use ipc_channel::ipc::channel;
pub mod message;
pub mod oplog;
pub mod coordinator;
pub mod participant;
pub mod client;
pub mod checker;
pub mod tpcoptions;
use message::ProtocolMessage;

///
/// pub fn spawn_child_and_connect(child_opts: &mut tpcoptions::TPCOptions) -> (std::process::Child, Sender<ProtocolMessage>, Receiver<ProtocolMessage>)
///
///     child_opts: CLI options for child process
///
/// 1. Set up IPC
/// 2. Spawn a child process using the child CLI options
/// 3. Do any required communication to set up the parent / child communication channels
/// 4. Return the child process handle and the communication channels for the parent
///
/// HINT: You can change the signature of the function if necessary
///
fn spawn_child_and_connect(child_opts: &mut tpcoptions::TPCOptions) -> (Child, Sender<ProtocolMessage>, Receiver<ProtocolMessage>) {
    // TODO
    debug!("coord::spawn_child_and_connect(): Setup temporary IPCOneShotServer for Coordinator to receive transmit channels from child");
    let (coord_server, coord_server_name) = IpcOneShotServer::new().unwrap();
    
    child_opts.ipc_path = coord_server_name;

    // NON-TODO
    let child = Command::new(env::current_exe().unwrap())
        .args(child_opts.as_vec())
        //.env("coord_server", coord_server_name) // modified to enable the process to access the name.  Warning: this is an environment variable, will other child command process overwrite it?  Initial design approach to setup the channels first before spawning another child, so maybe mitigated?
        .spawn()
        .expect("Failed to execute child process");

    // Assumption: If this code is provided, then assuming we'll have a dedicated channel from coordinator to child for every child.  They will not share a channel to send message to coordinator.
    let (tx, rx) = channel().unwrap();
    // TODO

    // Listen to OneShotServer to get child transmitters.
    let (_, (child_tx1, child_server_name)): (_, (Sender<ProtocolMessage>, String)) = coord_server.accept().unwrap();

    let child_tx0 = Sender::<Sender<ProtocolMessage>>::connect(child_server_name).unwrap();
    child_tx0.send(tx).unwrap();

    /*
    debug!("coord::spawn_child_and_connect(): Send message to child for testing");
    let test_message = ProtocolMessage::generate(message::MessageType::CoordinatorPropose, format!("7357"), format!("coord_0"), 7357);
    child_tx1.send(test_message).unwrap();

    debug!("coord::spawn_child_and_connect(): Receive test message from coordinator.");
    let test_message = rx.recv().unwrap();
    debug!("coord::spawn_child_and_connect(): {:?}", test_message);
    */

    (child, child_tx1, rx)
}

///
/// pub fn connect_to_coordinator(opts: &tpcoptions::TPCOptions) -> (Sender<ProtocolMessage>, Receiver<ProtocolMessage>)
///
///     opts: CLI options for this process
///
/// 1. Connect to the parent via IPC
/// 2. Do any required communication to set up the parent / child communication channels
/// 3. Return the communication channels for the child
///
/// HINT: You can change the signature of the function if necessasry
///
fn connect_to_coordinator(opts: &tpcoptions::TPCOptions) -> (Sender<ProtocolMessage>, Receiver<ProtocolMessage>) {
    let (tx, rx) = channel().unwrap();

    // TODO
    /*
    let key = "coord_server";
    let val = env::var(key).unwrap();
    debug!("{}::client::connect_to_coordinator(): {}:{}", opts.num, key, val);
    */
    debug!("{}::client::connect_to_coordinator(): {}", opts.num, opts.ipc_path);

    //debug!("{}::client::connect_to_coordinator(): Setup temporary IPCOneShotServer for child to receive transmit channels from coordinator", opts.num);
    let (child_server, child_server_name) = IpcOneShotServer::new().unwrap();

    // send to coordinater child transmitter and child IpcOneShotServer name
    let coord_tx0 = Sender::<(Sender<ProtocolMessage>, String)>::connect(opts.ipc_path.clone()).unwrap();
    coord_tx0.send((tx, child_server_name)).unwrap();

    // Receive coordinator transmitter.
    let (_, coord_tx1): (_, Sender<ProtocolMessage>) = child_server.accept().unwrap();
    
    /*
    debug!("{}::client::connect_to_coordinator(): Receive test message from coordinator.", opts.num);
    let test_message = rx.recv().unwrap();
    debug!("{}::client::connect_to_coordinator(): {:?}", opts.num, test_message);

    debug!("{}::client::connect_to_coordinator(): Send message to child for testing", opts.num);
    let test_message = ProtocolMessage::generate(message::MessageType::CoordinatorPropose, format!("7357"), format!("coord_0"), 7357);
    coord_tx1.send(test_message).unwrap();
    */

    (coord_tx1, rx)
}

///
/// pub fn run(opts: &tpcoptions:TPCOptions, running: Arc<AtomicBool>)
///     opts: An options structure containing the CLI arguments
///     running: An atomically reference counted (ARC) AtomicBool(ean) that is
///         set to be false whenever Ctrl+C is pressed
///
/// 1. Creates a new coordinator
/// 2. Spawns and connects to new clients processes and then registers them with
///    the coordinator
/// 3. Spawns and connects to new participant processes and then registers them
///    with the coordinator
/// 4. Starts the coordinator protocol
/// 5. Wait until the children finish execution
///
fn run(opts: & tpcoptions::TPCOptions, running: Arc<AtomicBool>) {
    let coord_log_path = format!("{}//{}", opts.log_path, "coordinator.log");

    // TODO
    debug!("coord::Execute run()");
    trace!("coord::run(): coord_log_path: {}", coord_log_path);
    let now = std::time::Instant::now();

    let total_num_requests = opts.num_clients * opts.num_requests;

    //debug!("coord::run(): Create a new coordinator");
    let mut coord = coordinator::Coordinator::new(coord_log_path, &running, total_num_requests);

    // Vector collection to collect child process handle
    let mut children_handler: Vec<(String, Child)> = Vec::new();

    //debug!("coord::run(): Spawn client child process");
    for i in 0..opts.num_clients {
        //debug!("coord::run(): Create client options based on coordinator options");
        let mut temp_opts = opts.clone();
        temp_opts.mode = "client".to_string();
        temp_opts.num = i;
        let process_name = format!("client_{}", i);

        //debug!("coord::run(): Spawn child and connect");
        let (child, child_tx, rx) = spawn_child_and_connect(&mut temp_opts.clone());

        let key = format!("client_{}", i);
        coord.client_join(&key, child_tx, rx);

        children_handler.push((process_name, child));
    }

    //debug!("coord::run(): Spawn participant child process");
    for i in 0..opts.num_participants {
        //debug!("coord::run(): Create participant options based on coordinator options");
        let mut temp_opts = opts.clone();
        temp_opts.mode = "participant".to_string();
        temp_opts.num = i;
        let process_name = format!("participant_{}", i);

        //debug!("coord::run(): Spawn child and connect");
        let (child, participant_tx, rx) = spawn_child_and_connect(&mut temp_opts.clone());

        let key = format!("participant_{}", i);
        coord.participant_join(&key, participant_tx, rx);

        children_handler.push((process_name, child));
    }

    coord.protocol();

    if !running.load(Ordering::Relaxed) {
        for child_handler in children_handler.iter_mut() {
            match child_handler.1.kill() {
                Ok(()) => info!("{}::Child process killed", child_handler.0),
                Err(error) => warn!("{}::Child.kill() produced an error: {}", child_handler.0, error),
            }
        }
    }

    info!("Wait for children to finish their process");
    for child_handler in children_handler.iter_mut() {
        match child_handler.1.wait() {
            Ok(r) => info!("{}::{}", child_handler.0, r),
            Err(error) => warn!("{}::Child.wait produced an error: {}", child_handler.0, error),
        }
    }

    let elapsed_time = now.elapsed().as_millis();
    info!("Total elapsed time of the coordinator and program: {}", elapsed_time);
}

///
/// pub fn run_client(opts: &tpcoptions:TPCOptions, running: Arc<AtomicBool>)
///     opts: An options structure containing the CLI arguments
///     running: An atomically reference counted (ARC) AtomicBool(ean) that is
///         set to be false whenever Ctrl+C is pressed
///
/// 1. Connects to the coordinator to get tx/rx
/// 2. Constructs a new client
/// 3. Starts the client protocol
///
fn run_client(opts: & tpcoptions::TPCOptions, running: Arc<AtomicBool>) {
    // TODO
    debug!("{}::client::run_client()", opts.num);

    //debug!("{}::client::run_client(): Connect to Coordinator to get tx/rx", opts.num);
    let (coord_tx, rx) = connect_to_coordinator(&opts);

    // Constructs a new client
    let num = format!("{}", opts.num);
    let mut client = client::Client::new(num, running, coord_tx, rx);

    // Starts the client protocol
    client.protocol(opts.num_requests);
}

///
/// pub fn run_participant(opts: &tpcoptions:TPCOptions, running: Arc<AtomicBool>)
///     opts: An options structure containing the CLI arguments
///     running: An atomically reference counted (ARC) AtomicBool(ean) that is
///         set to be false whenever Ctrl+C is pressed
///
/// 1. Connects to the coordinator to get tx/rx
/// 2. Constructs a new participant
/// 3. Starts the participant protocol
///
fn run_participant(opts: & tpcoptions::TPCOptions, running: Arc<AtomicBool>) {
    let participant_id_str = format!("participant_{}", opts.num);
    let participant_log_path = format!("{}//{}.log", opts.log_path, participant_id_str);

    // TODO
    debug!("{}::participant::main.run_participant()", opts.num);

     //debug!("{}::run_participant(): Connect to Coordinator to get tx/rx", opts.num);
     let (coord_tx, rx) = connect_to_coordinator(&opts);

     let total_num_requests = opts.num_clients * opts.num_requests;

     // Constructs a new participant
     let num = format!("{}", opts.num);
     let mut participant = participant::Participant::new(
        num, participant_log_path, 
        running, 
        opts.send_success_probability, 
        opts.operation_success_probability, coord_tx, 
        rx, 
        total_num_requests);
 
     // Starts the client protocol
     participant.protocol();
}

fn main() {
    // Parse CLI arguments
    let opts = tpcoptions::TPCOptions::new();
    // Set-up logging and create OpLog path if necessary
    stderrlog::new()
            .module(module_path!())
            .quiet(false)
            .timestamp(stderrlog::Timestamp::Millisecond)
            .verbosity(opts.verbosity)
            .init()
            .unwrap();
    match fs::create_dir_all(opts.log_path.clone()) {
        Err(e) => error!("Failed to create log_path: \"{:?}\". Error \"{:?}\"", opts.log_path, e),
        _ => (),
    }

    // Set-up Ctrl-C / SIGINT handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let m = opts.mode.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        if m == "run" {
            print!("\n");
        }
    }).expect("Error setting signal handler!");

    // Execute main logic
    match opts.mode.as_ref() {
        "run" => run(&opts, running),
        "client" => run_client(&opts, running),
        "participant" => run_participant(&opts, running),
        "check" => checker::check_last_run(opts.num_clients, opts.num_requests, opts.num_participants, &opts.log_path),
        _ => panic!("Unknown mode"),
    }
}
