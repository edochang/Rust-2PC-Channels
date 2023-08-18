# Rust Two-Phase Commit (2PC)
## Approach
The approach I took to program the Lab was to have a foundation of Rust Programming at a high-level and understand its programming idiom and its known challenges and differences with other programming languages (e.g., Type system and Memory / Variable Ownership).  From there, I proceeded to understand key concepts of the Lab - Inter-Process Communication (IPC) Channel Library and Two-Phase Commit.  After all that, practice through coding was next with the Lab.  

After spending time understanding the skeleton and Two-Phase Commit, I divided the skeleton into 5 key functionalities:
1.	Run Coordinator, Spawn Child Process, and Establish IPC Communication
2.	Run Client and Coordinator Communication
3.	Run Coordinator and Participant Communication
4.	Complete End-to-End (E2E) Communication 
5.	Enhance Termination or Exit sequences

### Run Coordinator, Spawn Child Process, and Establish IPC Communication
In this first part, I focused on setting up the Coordinator and Child Process to then build out the IPC communication channel for them to communicate with each other.  Because Processes usually have different memory area as compared to Threads, Processes will need to use some system IPC communication to share memory / data.  In this lab, we used IPC Channels to do this.  IPC Channels are like Rust Channels but are implemented over the native OS abstractions to enable inter-process communication.  From the IPC Channel library, I used an IpcOneShotServer on each side – child and coordinator, to exchange their transmitters.  This is done for each new child, meaning a dedicated two-way channel is created for each coordinator and child pair.  When spawning a child, the parent-coordinator arguments are cloned and modified to run client or participant child processes (opts.mode) using the same program code and an identifier is defined to differentiate client and participant processes (opts.num).  Once a child is spawned, depending on if it’s a client or participant, the child transmitter and dedicated coordinator receiver for the child is registered to a client or participant HashMap to be used in the 2PC protocol for sending and receiving 2PC message states.

### Run Client and Coordinator Communication
In this part, I used the client and coordinator communication process as the first functional test scenario to test out the IPC communication channel between coordinator and client child processes.  Once this works, the same design pattern that client processes and participant processes share can be replicated over.  

Design choices to call out in the coordinator implementation.
-	The coordinator uses a loop to process a request from each client.  A blocking receive method is used in a loop against the number of clients because the argument and lab requirements guarantees every client will send the same number of messages.  If client can send different number of requests from each other, then a non-blocking receive method would have been used to check every channel for messages until all channels are Empty.
-	ProtocolMessage.mtype was used to determine what state a transaction has gone through within the 2PC lifecycle.
-	Coordinator will log client request as “CoordinatorPropose” in its log.

Design choices to call out in the client Implementation.
-	Client protocol will send and receive a request from the Coordinator in a synchronous manner until all requests have been sent and processed by the coordinator.  
    -	If ctrl+c is signaled to stop processes from running, then the client will abort its loop to send the remaining request and will proceed to wait for exit signal.
-	The client will use a blocking receive method to simulate a synchronous transaction call and wait for the transaction to complete its Two Phase Commit (2PC) with the participants.  Like a travel agency use case where you might make a composite transaction call to book flights, hotels, and rental cars to complete a travel reservation.  If either flight system, hotel system, or rental car system fails to make reservation then do not commit any reservation transactions and notify the client system that reservation cannot be made.
-	The sum of the successful and abort count will equal to the total number of requests.  Unknown count will be a subset count of aborted transaction.  This provides insight into the aborted requests to understand how much of it is due to participant communication failure (a communication timeout where the coordinator doesn’t know what happened to a transaction / request from the participant side and is forced to do a global abort for all participants).  This unknown global abort is communicated back to the Client to count Unknowns.
-	Wait for exit signal will be based on two conditions.
    -	1) Coordinator signals “CoordinatorExit”
        -	Once Client enters this state when it’s done sending all requests and received results, a try receive method is used to continue to check if an exit signal has been sent by the Coordinator.  A non-blocking receiver is used instead because the client process also needs to check if running is still true, so it can shutdown due to the second condition.  A thread sleep timer is used to avoid spinning the processor resource to continue and check on the coordinator receiver for the exit signal.
        -	For the try receiver to work, the errors are carefully handled, so the process doesn’t panic and break.  The try receiver will only panic! / break the process if it receives IpcErrors that are not EMPTY or Disconnected.
            -	If it’s EMPTY, it means that the coordinator hasn’t sent anything yet.
            -	If it’s Disconnected, it means the coordinator has disconnected, but there could be messages in the buffer, so read the channel in the next loop around.
            -	If it’s anything else, something terrible from an OS level / infrastructure level has happened, so panic and break the program.
    -	2) ctrl+c updates running state to false.
        -	When this is false, the child will print its result and terminate itself.

### Run Coordinator and Participant Communication
With the Coordinator functionality built, the next part was to build the coordinator and participant logic to complete the end-to-end (E2E) communication.

Design choices to call out in the coordinator Implementation.
-	Send all client requests and global commits / aborts to each participant using a HashMap directory.
-	When receiving results, use a non-blocking receive that is timed.  When the time is hit, then treat the response as unknown and log the overall request as an unknown which will be treated as a global abort.
    -	Opportunity to improve this logic to optimize the performance but did not implement.  Store the start timer with each participant in the HashMap, so the coordinator can check the next channel without being blocked by a send failure from participant and can check the participant in the next loop around.  This is to avoid the situation where if more than one participant fails to send, then it increases the time a coordinator is stuck waiting based on the Time-To-Live / wait time setting for each participant, which we can observe in the next section of the report.
-	Sleep the coordinator when it’s waiting for the participant to send its vote.  The sleep timer will always be less than the timer, for example, 100 ms sleep timer for a wait timer of 500 ms (current hardcoded configuration in the program).
-	Coordinator will keep count of the votes.  If the vote commits equal to the total number of participants, then all participants will receive a global commit, otherwise a global abort will be sent.  A special “_unknown” flag will be tagged to txid if the transaction has an unknown transaction from any participant that failed to send a transaction to the coordinator.  This is to differentiate a vote abort and unknown transaction from a participant.  However, both vote aborts and unknown transactions will be considered as a global abort from the coordinator.
-	When all requests have been processed by the protocol, the coordinator will send “CoordinatorExit” to all child processes to start exiting when they’re done processing the protocol messages and are in wait for exit signal state.
-	All channel errors will be handled without throwing an exception (e.g. panic! or using unwrap() that will panic when a error is triggered) within the coordinator protocol.  Errors will be logged with error!() instead.  This is to ensure the parent / coordinator can kill child processes when ctrl+c is triggered which is handled at the end of the run() program.

Design choices to call out in the participant Implementation.
- Participant will use a blocking receive method for all its receiving message from coordinator.  This will naturally block and sleep the process when there’s no message in the channel.  
- The Participant will log its vote to be committed or aborted based on an operation success probability to abstractly represent a composite and complex transaction that could succeed or fail.
- The participant will send its vote based on a send success probability.  If successful it’ll send its vote.  Otherwise, it’ll log an error!() call to console for participant send failure.  Coordinator will timeout on receiving a message and move on to the next participant.
- Participant will enter wait for exit signal and have the same logic as client when waiting and exiting.  The only difference with client is that the participant will use total number of requests (number of requests * number of clients) to know it’ll not be receiving anymore messages and will go into a wait state, whereas client uses number of requests it’ll send to identify when it’s done and needs to go in a wait state.

### Complete End-to-End (E2E) Communication
Design choices to call out in the Coordinator Implementation.
-	Coordinator will tag a request as unknown and globally aborted when at least one participant fails to send a vote.  Coordinator will communicate this to the client requester and all participants.  Unknown tally will be based on this condition / scenario.
-	Verify successful, failed, and unknown operation counts are in-sync between client, coordinator, and participant.
    -	Client keeps track of the number of requests it sends and the status of each of those requests.
    -	Coordinator and participant keep tracks of all client transactions across all clients and the 2PC global commit and abort results.  This includes unknown counts where at least one participant fails to send.  Unknown count is based on number of transaction request has at least one participant send failure and not total number of participants that failed to send to coordinator.

        Example output below:
            
            edo@SD02-PCG:~/dev/cs_repo/Lab4/skeleton_2pc$ ./target/debug/two_phase_commit -s .95 -S .95 -c 4 -p 10 -r 10 -m run
            2022-11-16T22:01:06.650-06:00 - ERROR - participant_8::Fail to send to coordinator
            2022-11-16T22:01:07.394-06:00 - ERROR - participant_8::Fail to send to coordinator
            2022-11-16T22:01:08.142-06:00 - ERROR - participant_2::Fail to send to coordinator
            2022-11-16T22:01:08.883-06:00 - ERROR - participant_6::Fail to send to coordinator
            2022-11-16T22:01:09.865-06:00 - ERROR - participant_7::Fail to send to coordinator
            2022-11-16T22:01:10.518-06:00 - ERROR - participant_1::Fail to send to coordinator
            2022-11-16T22:01:10.518-06:00 - ERROR - participant_8::Fail to send to coordinator
            2022-11-16T22:01:11.605-06:00 - ERROR - participant_7::Fail to send to coordinator
            2022-11-16T22:01:12.366-06:00 - ERROR - participant_7::Fail to send to coordinator
            2022-11-16T22:01:13.236-06:00 - ERROR - participant_4::Fail to send to coordinator
            2022-11-16T22:01:14.108-06:00 - ERROR - participant_0::Fail to send to coordinator
            2022-11-16T22:01:14.108-06:00 - ERROR - participant_3::Fail to send to coordinator
            2022-11-16T22:01:15.387-06:00 - ERROR - participant_3::Fail to send to coordinator
            2022-11-16T22:01:16.134-06:00 - ERROR - participant_6::Fail to send to coordinator
            2022-11-16T22:01:16.771-06:00 - ERROR - participant_6::Fail to send to coordinator
            2022-11-16T22:01:17.530-06:00 - ERROR - participant_7::Fail to send to coordinator
            2022-11-16T22:01:18.182-06:00 - ERROR - participant_3::Fail to send to coordinator
            2022-11-16T22:01:18.183-06:00 - ERROR - participant_5::Fail to send to coordinator
            2022-11-16T22:01:19.371-06:00 - ERROR - participant_3::Fail to send to coordinator
            2022-11-16T22:01:20.334-06:00 - ERROR - participant_2::Fail to send to coordinator
            participant_3   :       Committed:     17       Aborted:     23 Unknown:     17
            coordinator     :       Committed:     17       Aborted:     23 Unknown:     17
            participant_0   :       Committed:     17       Aborted:     23 Unknown:     17
            client_2        :       Committed:      4       Aborted:      6 Unknown:      5
            participant_8   :       Committed:     17       Aborted:     23 Unknown:     17
            participant_7   :       Committed:     17       Aborted:     23 Unknown:     17
            participant_9   :       Committed:     17       Aborted:     23 Unknown:     17
            participant_4   :       Committed:     17       Aborted:     23 Unknown:     17
            participant_5   :       Committed:     17       Aborted:     23 Unknown:     17
            client_1        :       Committed:      5       Aborted:      5 Unknown:      5
            client_3        :       Committed:      3       Aborted:      7 Unknown:      3
            client_0        :       Committed:      5       Aborted:      5 Unknown:      4
            participant_2   :       Committed:     17       Aborted:     23 Unknown:     17
            participant_6   :       Committed:     17       Aborted:     23 Unknown:     17
            participant_1   :       Committed:     17       Aborted:     23 Unknown:     17

See graph below for a visual representation of unknowns and aborted transactions.

![Transaction_Based_on_Operation_Success_Probability](/markdown_assets/Transaction_Based_on_Operation_Success_Probability.png)

As you can see unknown will equal or be a subset of aborted based on my implementation logic.  So in this example, out of the 26 aborted transactions, 19 are due to unknow states from participants due to a send failure and coordinator timing out on the receiver.

### Enhance Termination and Exit Sequences
Design choices to call out in this implementation.
-	Store all Child Command Process Handlers in a vector of tuples (String, Child Process Handler).  The Vector will be used as a list to loop through to either kill a child process or wait for a child process to complete.
-	If a ctrl+c is initiated and running is false, the parent coordinator will exit its loop based on the running condition and eventually return to run() to then kill all child processes.
-	If the parent coordinator program is allowed to run to the end, it’ll wait for each child process to finish before exiting.  This is to avoid triggering channel IpcErrors of disconnect, since I did not handle disconnection errors gracefully in client or participant.  It is only handled gracefully in the coordinator process, to avoid the parent coordinator program from throwing an exception either through panic! or unwrap(), so it can run to the end to execute these clean-up process to trigger a signal kill of child processes.

## Present Performance Results
We will use a baseline configuration to observe the program performance and behavior.  The baseline will be:

|Option                     |Value|
|---------------------------|-----|
|Client                     |4    |
|Participant                |10   |
|Number of Client Requests  |10   |

The baseline has the following average run time with the following probability set.

|Configuration               |No Probability|-s .95  |	-s .95 , -S .95|
|----------------------------|--------------|--------|-----------------|
|Average Execution Time (ms) |	5345.67	    |5308.33 |	19435.00       |

Based on the observation above, we can see that the timeout approach that was implemented between coordinator and participant adds time to the overall execution time, because for every failure send, the full 500 ms wait time is used before the coordinator gives up and moves to the next participant.

We can further observe this behavior when we decrease the send success probability.  As you can see in the diagram below, the execution time increases as the coordinator experiences more send failures from the participant.

![Execution_Time_Send_Success_Probability](/markdown_assets/Execution_Time_Send_Success_Probability.png)

If we use the baseline execution time as a frame of reference using client = 4, participant = 10, requests = 10 and modify each dimension to see it’s impact to execution time, we will observe the following behavior in the graph below.

![Execution_Time_Modification_of_One_Dimension](/markdown_assets/Execution_Time_Modification_of_One_Dimension.png)

Notable callouts are:
-	When you change requests or clients but leave either one of them the same, you’ll still get similar execution times since the total request is 80 that’s being processed by the coordinator and participant (8 client * 10 requests = 80 vs. 4 clients * 20 requests = 80). 
-	Another behavior worth noting and is a trade-off based on the approach and design choices made on the coordinator timeout on receiving messages from participants.  If you increase participants to 50 and leaving the rest of the dimensions as baseline value of 4 clients and 10 requests, you see improvement in execution time.  This is a consequence from a design choice to sleep the coordinator thread when it doesn’t receive anything from the try receive channel.  However, this consequence also sheds some light on how fast the try receive channel is if it’s not blocking like the regular receive channel method.  Therefore, we see performance improvements when coordinator is sending messages to more participants because after the coordinator has sent message to all 50 participants, some participants have already queued a vote response back to coordinator without invoking the thread sleep method.  
Note: left my implementation with the 100 ms thread sleep and 500 time out for the TA to also observe this extreme behavior, but to improve grading speed, it’s worth for the TA to update the thread sleep timer to be 20 ms or 30 ms.  I could have expanded an argument / option for the TA to modify this setting.
-   In addition, if we increase the number of requests to the coordinator and participant leaving participants at 10, we can see an increase in execution time.  This is due to the thread sleep design choice observed in the bullet above, but also the design choice to run a synchronous request and response pattern between client and coordinator.  Though the increase of this time is not as impactful as the thread sleep time.  From a performance standpoint, this is the major bottleneck, compared to increases the number of requests.  This can be observed if we change all parameters to increase the number of requests while also increasing the number of participants to minimize the impacts of the thread sleep timer of 100 ms.

    ![Compare_Execution_Time_160_and_2000_Requests](/markdown_assets/Compare_Execution_Time_160_and_2000_Requests.png)

    As you can see in the chart above, we can process 2000 requests in almost the same amount if not better time than processing only 160 requests with 10 participants.  For real-world use cases we wouldn’t use such a long thread sleep timer, but this shows the tradeoff decision you will face if you’re prioritizing processor utilization over processing speed for any given number of requests.  For more practical uses, the thread sleep timer would be somewhere less than 100 ms while balancing how fast the try receive method is to better manage and balance process utilization.

### Operation and Send Success Probability
The abstracted operation and send success probability have similar behaviors.  As the probability decreases, the more individual participant transactions fail.  If 1 fails (either due to operations or communication), the whole 2PC composite transaction fails.

![Transaction_Based_on_Operation_Success_Probability_2](/markdown_assets/Transaction_Based_on_Operation_Success_Probability_2.png)
![Transaction_Based_on_Operation_Success_Probability_3](/markdown_assets/Transaction_Based_on_Operation_Success_Probability_3.png)

## Insights
### Rust Ownership
Rust ownership model was difficult to master at first but reading up documentation and explanation on key ownership behaviors helped with understanding how to effectively manage memory and references.  It also helped in understanding how to use standard or community libraries (e.g., Self vs. &Self)
Key takeaways:
-	Share Ownership where possible; otherwise, clone or copy the data.
-	Read each Rust library to see if the objects support clone or copy.  
-	If clone or copy isn’t supported, determine if the data / memory is truly needed in subsequent areas of the program. 
-	Ownership isn’t only about your implementation of sharing / passing memory, but also about others library implementation.  Knowing how ownership is passed in parameters will help you understand object behaviors in those libraries.

### Rust Type System and Inference Abilities
Had challenges with the Rust Type system and the rustc Analyzer and how it can infer types.  If a type is defined elsewhere in the code within the same block {}, then the type can be inferred.  Otherwise, if it cannot be inferred, rustc will throw errors to challenge the programmer to be explicit.  

### SIGINT Handlers and Behavior
Had challenges ensuring processes naturally terminated without using the Child Process Handler, when ctrl+c is triggered.  I didn’t realize the SIGINT handler overrides ctrl+c termination behaviors if a custom handler is defined.  I had to implement logic to explicitly kill processes to ensure zombie processes were not running, which is a behavior I’m use to with the default ctrl+c behavior when a custom SIGINT handler isn’t defined.  This also impacted some of the design choices I made in the coordinator error handling logic to ensure my program dies gracefully when it finishes running or when it is interrupted with a ctrl+c signal.

### Self vs &Self
When reading the IpcOneShotServer example: https://github.com/servo/ipc-channel/blob/master/src/test.rs#L257.  At first, I was confused why the test case was passing multiple IpcOneShotServers.  Because it was sending another one across the process, I thought it was a traditional server that can take on more than one request.  However, when I tried this in a hello world example, it didn’t work as expected.  It kept dying after one use.  After reading the IpcOneShotServer implementation and understanding ownership, seeing that it passes itself, I realize it deconstructs itself once it can accept a message from the sender.

    pub fn accept(self) -> Result<(IpcReceiver<T>, T), Error>

Again, understanding Rust ownership and how it deallocates memory after a code block {}, helped me better understand how to use libraries and their methods.

### Error Handling and Panics
Handling Errors was complex and time consuming for me.  There were different ways of doing it and each had it nuances.  You can use match expressions on its own or use match expression that returns data.  Or try using conditional statements by using error methods.  I tried different ways to handle errors to get a feel for them.  Ultimately, I’d like how match expression works especially when I realized that the lambda expression wasn’t just for one line expressions but can be a whole lambda function code block {}.  Also learned that you can ensure whether unwrap() on a Result or Option returns a result or not by using .is_some(), .is_none(), is_err() methods.  These methods were handy to explicitly understand what you’ll be handling before you unwrap() so not to cause a panic.

# Appendix

## Computer Information
AMD Ryzen 9 5900HS with Radeon Graphics, 3301 Mhz, 8 Core(s), 16 Logical Processor(s)
Installed Physical Memory (RAM): 24.0 GB

### lscpu on WSL2 

![lscpu](/markdown_assets/lscpu.png)

### OS Version 
Windows 11 Pro, Version 22H2, OS Build 22621.608
Windows Subsystem for Linux Distributions:  Ubuntu-20.04

    uname -r >>> 5.15.57.1-microsoft-standard-WSL2

### Rust Version 
    rustc --version
    rustc 1.64.0 (a55dd71d5 2022-09-19)

## Running The Program
Run the following in the terminal:

    ./target/debug/two_phase_commit --ipc_path ./ipc/ -c 4 -p 10 -r 10 -m run

**./target/debug/two_phase_commit Argument Instructions**
    
    ./target/debug/two_phase_commit --help
    concurrency-2pc 0.1.0
    Rust Student <edward.chang@utexas.edu>
    2PC exercise written in Rust

    USAGE:
        two_phase_commit [OPTIONS]

    FLAGS:
        -h, --help       Prints help information
        -V, --version    Prints version information

    OPTIONS:
            --ipc_path <ipc_path>                 Path for IPC socket for communication
        -l <log_path>                             Specifies path to directory where logs are stored
        -m <mode>                                 Mode: "run" starts 2PC, "client" starts a client process, "participant"
                                                starts a participant process, "check" checks logs produced by previous run
            --num <num>                           Participant / Client number for naming the log files. Ranges from 0 to
                                                num_clients - 1 or num_participants - 1
        -c <num_clients>                          Number of clients making requests
        -p <num_participants>                     Number of participants in protocol
        -r <num_requests>                         Number of requests made per client
        -s <operation_success_probability>        Probability participants successfully execute requests
        -S <send_success_probability>             Probability participants successfully send messages
        -v <verbosity>                            Output verbosity: 0->No Output, 5->Output Everything

### Build the Program
Run the following in the terminal:

    cargo build