// STEP 0: Import necessary modules and types
use std::{
    collections::HashMap, string::FromUtf16Error, sync::{
        atomic::{AtomicBool, Ordering as AtomicOrdering}, mpsc::{self, Receiver, Sender}, Arc, Mutex // For shutdown signal
    }, thread
};
use tauri::{AppHandle, Emitter}; // To communicate with the frontend


use windows::{
    core::{implement, Interface, Ref, Result, BOOL, GUID, PCWSTR, PWSTR}, // Core Windows interface types
    Win32::{Foundation::{CloseHandle, HANDLE}, Media::Audio::{
        eConsole, eRender, AudioSessionDisconnectReason, AudioSessionState, AudioSessionStateActive, AudioSessionStateExpired, AudioSessionStateInactive,
        Endpoints::IAudioEndpointVolume, IAudioSessionControl, IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionEvents, 
        IAudioSessionEvents_Impl, IAudioSessionManager2, IAudioSessionNotification, IAudioSessionNotification_Impl, 
        IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator // audio session states
     }, System::{Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED // COM threading model (apartment threaded)
     }, Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS 
     },},}
};

use tauri::Manager;
use tokio::net::{TcpListener, TcpStream}; // Provides the TCP listener for incoming connections.
use futures_util::stream::StreamExt; // Extension trait for working with streams (like incoming messages).
use futures_util::sink::SinkExt; // Extension trait for sending messages (sinking data).
use std::net::SocketAddr; // Standard type for storing IP addresses and ports.
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Error}; // Core type definitions for the async WebSocket stream handler.
use tokio_tungstenite::tungstenite::Message; // Type used to represent a WebSocket frame (Text, Binary, Ping, Close, etc.).
use tokio::time::{sleep, Duration};

fn get_process_name_by_id(process_id: u32) -> Result<Option<String>> {


    unsafe {
        // Create a snapshot of all processes.
        // This is a point-in-time capture of all running processes in the system
        let snapshot: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        // Check if the snapshot is valid
        if snapshot.is_invalid() {
            return Ok(None);
        }



        // Prepare process_entry to use it later to hold information.
        // PROCESSENTRY32W: Windows API structure that holds information about a process, including its executable name, process ID, and other attributes. 
        //It's commonly used with functions like Process32FirstW and Process32NextW 
        let mut process_entry = PROCESSENTRY32W {
            // dwSize: represents the size of the PROCESSENTRY32W structure in bytes. it's mandatory to set dwSize to the size of the structure.
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, // computes the size of the PROCESSENTRY32W structure at compile time. 
            ..Default::default() //  initializes all other fields of the PROCESSENTRY32W structure to their default values.
        };



        // here we will use snapshot wich holds all the process and then we get the first process info by using Process32FirstW and then storing the info to process_entry.
        // Process32FirstW: Retrieves information about the first process encountered in a system snapshot.
        Process32FirstW(
            snapshot, // A handle to the snapshot obtained from CreateToolhelp32Snapshot (all the snapshots captured with "let snapshot: HANDLE" )
            &mut process_entry // A mutable reference to a PROCESSENTRY32W structure that will receive information about the process. (the thing that will hold info)
        )?;


        // Determines if the process retrieved matches the target process ID. (process_id IS THE ID OF THE PROCESS THAT OWNS THE AUDIO SESSION that we got in the code above).
        if process_entry.th32ProcessID == process_id { // we check if The process ID of the current process in the snapshot == The target process ID we're searching for.

            // Converts the process's executable name from UTF-16 to a Rust String.
            let name_slice: Vec<u16> = process_entry // (at this point process_entry holds the name of the process that we need to convert to rust string with the next steps below:)

                .szExeFile // .szExeFile makes process_entry a fixed-size array representing the process name in UTF-16.

                .iter() // Iterate over process_entry.szExeFile wich is a fixed-size array

                .take_while(|&&c| c != 0) // collect characters until the null terminator (\0) is encountered.

                .copied() // ensures that the collected characters are owned and not references.

                .collect(); // gathers them into a Vec<u16>.

            // now at this point name_slice is a Vec<u16> of collected characters that represent the process name that we need to convert to a rust string

            let exe_name = String::from_utf16_lossy(&name_slice); // converts the UTF-16 vector to a Rust String, replacing any invalid characters with the Unicode replacement character.

            // Releases the handle to the snapshot to free system resources.
            let _ = CloseHandle(snapshot); // this returns a result so it can fail but the fail is not critical so we ignore it by assignint it to let _

            // Returns the name of the executable if the process ID matches.
            return Ok(Some(exe_name)); // Wraps the executable name in Some and then in Ok to indicate a successful result.
        }



        
        // This loop iterates over the remaining processes in the snapshot.
        // Process32NextW advances to the next process in the snapshot and fills in process_entry with its details.
        while Process32NextW(snapshot, &mut process_entry).is_ok() {// .is_ok() checks if the call succeeded. When there are no more processes or an error occurs, the loop ends

            // we do the same thing we did above jsut this time in a loop
            if process_entry.th32ProcessID == process_id { // If it matches, the target process has been found; if not, the loop continues with the next process.

                let name_slice: Vec<u16> = process_entry
                    .szExeFile
                    .iter()
                    .take_while(|&&c| c != 0)
                    .copied()
                    .collect();
                let exe_name = String::from_utf16_lossy(&name_slice);
                let _ = CloseHandle(snapshot);
                return Ok(Some(exe_name)); // This exits the function immediately, so no further processes are checked.
            }
        } // If no match is found in the current iteration, the loop continues with the next process.

        // No matching process found.
        let _ = CloseHandle(snapshot);//After the loop finishes (i.e., when there are no more processes in the snapshot or an error occurs), the code closes the snapshot handle (free resoursses).
        Ok(None) // It then returns Ok(None), indicating that no process with the specified process_id was found.
    }
}









#[derive(Debug, serde::Serialize, Clone)] // For serializing session details to JSON
// Struct to hold session details to send to the frontend
pub struct SessionDetails {
    #[serde(rename = "pid")] // <-- RENAME FOR SERIALIZATION
    pub process_id: u32,

    #[serde(rename = "uid")] // <-- RENAME FOR SERIALIZATION
    pub session_uid: String,

    #[serde(rename = "name")] // <-- RENAME FOR SERIALIZATION
    pub process_name: String,

    #[serde(rename = "volume")] // <-- RENAME FOR SERIALIZATION
    pub session_volume: f32,

    pub is_muted: bool,
    pub is_active: bool,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct VolumeChangedPayload {
    // Use serde to rename the field to match the frontend's expectation 
    #[serde(rename = "uid")]
    session_uid: String,
    #[serde(rename = "newVolume")]
    volume: f32,
    #[serde(rename = "isMuted")]
    is_muted: bool, 
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct SessionStatePayload {
    #[serde(rename = "uid")]
    session_uid: String,
    #[serde(rename = "is_active")]
    is_active: bool,
}


enum MonitorThreadMessage {
    SessionCreated(IAudioSessionControl),
    ExistingSessionClosed(String),
}




#[implement(IAudioSessionEvents)]
struct IndividualSessionEventsListener {
    session_uid: String,
    app_handle: AppHandle,
    monitor_loop_sender: Sender<MonitorThreadMessage>,
}

#[allow(non_snake_case)]
impl IAudioSessionEvents_Impl for IndividualSessionEventsListener_Impl {
    fn OnDisplayNameChanged(&self, _new_display_name: &PCWSTR, _event_context: *const GUID) -> Result<()> { 
        Ok(()) 
    }

    fn OnIconPathChanged(&self, _new_icon_path: &PCWSTR, _event_context: *const GUID) -> Result<()> { 
        Ok(()) 
    }

    fn OnChannelVolumeChanged(&self, _channel_count: u32, _new_channel_volume_array: *const f32, _changed_channel: u32, _event_context: *const GUID) -> Result<()> {
        Ok(()) 
    }

    fn OnGroupingParamChanged(&self, _new_grouping_param: *const GUID, _event_context: *const GUID) -> Result<()> { 
        Ok(()) 
    }

    
    fn OnSimpleVolumeChanged(&self, new_volume: f32, new_mute: BOOL, _event_context: *const GUID) -> Result<()> {
        let payload = VolumeChangedPayload {
            session_uid: self.session_uid.clone(),
            volume: new_volume,
            is_muted: new_mute.as_bool(), 
        };
        println!("[AudioMonitor] Emitting 'audio-session-volume-changed' for session: {}", payload.session_uid);
        self.app_handle.emit("audio-session-volume-changed", payload).unwrap_or_else(|e| {        
            eprintln!("Failed to emit volume change event: {:?}", e);
        });

        Ok(()) 
    }
    #[allow(non_upper_case_globals)] // Allow this lint for the duration of the function
    fn OnStateChanged(&self, new_state: AudioSessionState ) -> Result<()> {

        match new_state {
            AudioSessionStateExpired => {
                self.app_handle.emit("audio-session-closed", &self.session_uid).unwrap_or_else(|e| {
                    eprintln!("[AudioMonitor] Error emitting session closed event for {}: {}", self.session_uid, e);
                });
                
                if self.monitor_loop_sender.send(MonitorThreadMessage::ExistingSessionClosed(self.session_uid.clone())).is_err() {
                    eprintln!("[AudioMonitor] Failed to send session closed message for {} to main loop.", self.session_uid);
                }
            }
            AudioSessionStateActive => {
                let payload = SessionStatePayload {
                    session_uid: self.session_uid.clone(),
                    is_active: true,
                };
                self.app_handle.emit("session-state-changed", payload).unwrap_or_else(|e| {
                    eprintln!("[AudioMonitor] Failed to emit active state for Session {}: {}", self.session_uid, e)
                });
            }
            AudioSessionStateInactive => {
                let payload = SessionStatePayload {
                    session_uid: self.session_uid.clone(),
                    is_active: false
                };
                self.app_handle.emit("session-state-changed", payload).unwrap_or_else(|e| {
                    eprintln!("[AudioMonitor] Failed to emit inactive state for Session {}: {}", self.session_uid, e)
                })
            }
            _ => {
                eprintln!("[AudioMonitor] Session '{}' received an unknown state: {:?}", self.session_uid, new_state.0);
            }
        }

        Ok(())
    }

    fn OnSessionDisconnected(&self, _disconnect_reason: AudioSessionDisconnectReason) -> Result<()> {
        self.app_handle.emit("audio-session-closed", &self.session_uid).unwrap_or_else(|e| {
            eprintln!("[AudioMonitor] Error emitting session disconnected event for {}: {}", self.session_uid, e);
        });

        if self.monitor_loop_sender.send(MonitorThreadMessage::ExistingSessionClosed(self.session_uid.clone())).is_err() {
            eprintln!("[AudioMonitor] Failed to send session closed message for {} to main loop.", self.session_uid);
        }

        Ok(())
    }
}


#[implement(IAudioSessionNotification)]
struct GlobalAudioSessionNotifier  {
    monitor_loop_sender: Sender<MonitorThreadMessage>,
}
#[allow(non_snake_case)]
impl IAudioSessionNotification_Impl for GlobalAudioSessionNotifier_Impl  { 
    // --- THIS IS THE KEY METHOD called by Windows when a NEW session is created ---
    // This callback runs on a Windows thread. Send the new session to the main loop for processing.
    fn OnSessionCreated(&self, new_session_control: Ref<'_, IAudioSessionControl>) -> Result<()> { 
        // here for whatever reason new_session_control goes from type Ref<'_, IAudioSessionControl> to Option<IAudioSessionControl> hence why we handle Some()
        if let Some(owned_session_control)  = new_session_control.clone() {
            if self.monitor_loop_sender.send(MonitorThreadMessage::SessionCreated(owned_session_control)).is_err(){
                eprintln!("[AudioMonitor] GlobalNotifier: Failed to send new session to main monitor loop.");
            }
        }
        Ok(())
    }
}

unsafe fn take_pwstr_to_string(pwstr: PWSTR) -> Option<String> {

    if pwstr.is_null(){
        return None; // if pwstr is null we return None
    }    

    let owned_string = pwstr.to_string() // convert to string
    .ok()
    .filter(|s| !s.is_empty()); // check if the string is empty and returns None if it is

    CoTaskMemFree(Some(pwstr.as_ptr() as _)); // <- Frees the memory here
    
    owned_string
     
}

fn get_session_details(session_control: &IAudioSessionControl) -> Result<SessionDetails> {
    let session_volume_control: ISimpleAudioVolume = session_control.cast()?;
    let volume = unsafe {
        session_volume_control.GetMasterVolume()?
    };
    let muted = unsafe {
        session_volume_control.GetMute()?
    };

    #[allow(non_upper_case_globals)]
    let active = unsafe {session_control.GetState()}.map(|state| {
        match state {
            AudioSessionStateActive => true,
            _ => false, // here we only really need the AudioSessionStateInactive to be false but since we need to exhaust every other possible arm we group them all to be false 
        }
    })?;


    let session_control2: IAudioSessionControl2 = session_control.cast()?;
    let id = unsafe {
        session_control2.GetProcessId().unwrap_or(0)
    };

    let uid = unsafe {
        let pwstr = session_control2.GetSessionInstanceIdentifier()?;
        match take_pwstr_to_string(pwstr) { // this converts and frees the pwstr
            Some(uid) => uid,
            None => {
                eprintln!("[AudioMonitor] Could not get a valid UID for a session. Skipping it.");
                return Err(windows::core::Error::new(windows::Win32::Foundation::E_FAIL, "Session has no instance identifier."));
            }
        } 
    };
    
/* 
    let display_name = match pwstr_to_owned_string(display_name_pwstr).ok().flatten() {
        Some(name) => name,
        None => match get_process_name_by_id(id).ok().flatten() {
            Some(name_by_id) => name_by_id,
            None => "Unknown".to_string(),
        }
    };
*/
    let display_name: String  = unsafe {
        let pwstr = session_control2.GetDisplayName()?;
        take_pwstr_to_string(pwstr)
    }
    .or_else(|| get_process_name_by_id(id).ok() // here if pwstr is none or string is empty we use this function to get another name
    .flatten())
    .unwrap_or_else(|| "unknown".to_string());
    


    Ok(SessionDetails { process_id: id, session_uid: uid, process_name: display_name, session_volume: volume, is_muted: muted.as_bool(), is_active: active })

}

// loop that runs ina thread and catchs new opened or removed process audio instances
pub fn monitor_thread_loop(app_handle: AppHandle, shutdown_signal: Arc<AtomicBool>)  {

    unsafe { 
        if let Err(e) = CoInitializeEx( None, COINIT_MULTITHREADED ).ok() {
            eprintln!("[AudioMonitor] COM failed to initialize: {:?}", e);
            return;
        }
    }

    // To safely communicate between the Windows callback threads and our main monitor loop thread,
    // we use an MPSC channel. The sender is for the callbacks, the receiver is for the loop.
    let (monitor_loop_sender, monitor_loop_receiver) = mpsc::channel::<MonitorThreadMessage>();



    // This will hold the single, global IAudioSessionNotification COM object after it's successfully registered.
    // We store it here for two critical reasons:
    // 1. Lifetime Management: Keeping it in this variable ensures the object stays alive for the entire
    //    duration of the monitor thread, so Windows can call its `OnSessionCreated` method.
    // 2. Cleanup: We need to access this exact object again during shutdown to unregister it.
    // It's an `Option` because registration can fail, in which case it will remain `None`.
    let mut global_notifier_com_object: Option<IAudioSessionNotification> = None;  


    // This HashMap tracks all the individual audio sessions we are currently monitoring.
    // - Key: The session's unique instance identifier (`session_uid`), which is a String.
    //        This is the only way to reliably distinguish between different audio sessions.
    // - Value: A tuple containing the live COM objects for that specific session:
    //          1. `IAudioSessionControl`: Needed to unregister the listener.
    //          2. `IAudioSessionEvents`: The listener object itself, which receives callbacks
    //                                   like OnStateChanged and OnSimpleVolumeChanged.
    // We store these objects here to keep them alive and to manage their lifecycle (adding new
    // listeners for new sessions and removing/unregistering them when sessions close).
    let mut active_individual_listeners: HashMap<String, (IAudioSessionControl, IAudioSessionEvents)> = HashMap::new();





    let device_enumerator: IMMDeviceEnumerator = unsafe {
        match CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Failed to create MMDeviceEnumerator instance: {:?}", e);
                    return
                }
            }
    };

    let default_device = unsafe {
        match device_enumerator.GetDefaultAudioEndpoint(
            eRender,
            eConsole,) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Failed to get DefaultAudioEndpoint: {:?}", e);
                    return
                }
            }
    };

    let session_manager: IAudioSessionManager2 = unsafe {
        match default_device.Activate(
            CLSCTX_ALL,
            None,) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to activate IAudioSessionManager2 on default device: {:?}", e);
                    return
                }
            }
    };

    // We register COM listeners that Windows will call when audio session events happen.
    // The main loop below processes the results from these listeners.
    // (The COM callbacks are executed on separate, Windows-managed threads.)

    // create an instance of the struct and convert it to a com object using .into()
    let global_notifier: IAudioSessionNotification = GlobalAudioSessionNotifier { monitor_loop_sender: monitor_loop_sender.clone() }.into();
    // register this com object with RegisterSessionNotification so that windows knows that it exists 
    if unsafe { session_manager.RegisterSessionNotification(&global_notifier).is_ok() } {
        // store it in a variable so that it keeps it alive because global_notifier will go out of scope outside the if statement
        global_notifier_com_object = Some(global_notifier); 
    } else {
        eprintln!("[AudioMonitor] CRITICAL: Failed to register global session notifier.");
    }

    
    let session_enumerator: IAudioSessionEnumerator = unsafe { 
        match session_manager.GetSessionEnumerator() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to get SessionEnumerator: {:?}", e);
                return
            }
        }
    };

    let session_count = unsafe { 
        match session_enumerator.GetCount() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to get session count: {:?}", e);
                return
            }
        }
    };
        
    for i in 0..session_count {

        let session_control: IAudioSessionControl = unsafe { 
            match session_enumerator.GetSession(i) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("Failed to Get Session i: {:?}", e);
                    continue // if one session fails we skip it to the next session in the loop
                }
            }
        };

        let details = match get_session_details(&session_control) {
            Ok(details) => details,
            Err(e) => {
                eprintln!("Error: {:?}", e);
                continue // if one session fails to get details we again skip to the next
            }
        };

        // create an instance of the struct and convert it to a com object using .into()
        let listener: IAudioSessionEvents = IndividualSessionEventsListener{
            session_uid: details.session_uid.clone(),
            app_handle: app_handle.clone(),
            monitor_loop_sender: monitor_loop_sender.clone(),
        }.into();

        // register this com object with RegisterAudioSessionNotification so that windows knows that it exists 
        if unsafe { session_control.RegisterAudioSessionNotification(&listener).is_ok()} {
            // store it in a hashmap so that it keeps it alive because global_notifier will go out of scope outside the if statement
            active_individual_listeners.insert(details.session_uid.clone(), (session_control, listener));
            // for each session i now we have registered it with com notifications when a change happens to it it will be detected
            // as for these sessions getting to the frontend that is handelled by a tauri::command
     
        }
    };

  
    // The main loop receives and processes event data sent by the COM callbacks.
        // this loop is responsible for doing the same things we did to the already existing sessions in the previous code to new created sessions.
    loop {


        // For shutdown, we use a shared AtomicBool (`shutdown_signal`), not the MPSC channel.
        // The main Tauri thread sets it to true, and this loop checks it to know when to exit.
        if shutdown_signal.load(AtomicOrdering::Relaxed) {
            break;

        }

        match monitor_loop_receiver.try_recv() {
            Ok(MonitorThreadMessage::SessionCreated(session_control)) => {
                let details = match get_session_details(&session_control) {
                    Ok(details) => if active_individual_listeners.contains_key(&details.session_uid) {
                        continue;
                    }else {details}
                    Err(e) => {
                        eprintln!("Error: {:?}", e);
                        continue
                    }
                };
                let listener : IAudioSessionEvents = IndividualSessionEventsListener{
                    session_uid: details.session_uid.clone(),
                    app_handle: app_handle.clone(),
                    monitor_loop_sender: monitor_loop_sender.clone(),
                }.into();
                if unsafe {session_control.RegisterAudioSessionNotification(&listener).is_ok()} {
                    active_individual_listeners.insert(details.session_uid.clone(), (session_control, listener));
                    println!("[AudioMonitor] Emitting 'audio-session-created' for session: {}", details.session_uid);
                    app_handle.emit("audio-session-created", &details).unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                    });
                }
            }

            Ok(MonitorThreadMessage::ExistingSessionClosed(closed_session_uid)) => {
                if let Some((control,listener)) = active_individual_listeners.remove(&closed_session_uid) {
                    unsafe {let _ = control.UnregisterAudioSessionNotification(&listener);}
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                eprintln!("[AudioMonitor] MPSC channel disconnected. Exiting loop.");
                break;
            }

        }

        thread::sleep(std::time::Duration::from_millis(200));
        
    }

    if let Some(global_notifier) = global_notifier_com_object.take() {
        unsafe { let _ = session_manager.UnregisterSessionNotification(&global_notifier); }
    }
    for (_id, (control, listener)) in active_individual_listeners.drain() {
        unsafe { let _ = control.UnregisterAudioSessionNotification(&listener); }
    }
    unsafe { CoUninitialize(); }

}



// start a websocket server that connects and listens for audio info from the browser extension
// we pass it app_handle to use it to send audio updates from the extension to the application UI
pub async fn websocket_server(app_handle: AppHandle, shutdown_signal: Arc<AtomicBool> ) {
    let port = "127.0.0.1:8080";
    // open a channel in this port to listen to
    let listener = match TcpListener::bind(port).await {
        Ok(listener) => listener,
        Err(e) => {
            // Log to terminal
            eprintln!("Fatal Server Error: {}", e);
            let error_msg = format!("Port is already in use: {}", port);
            // Tell the Vue UI so the user knows why it's not working
            app_handle.emit("server-error", error_msg).unwrap_or_else(|e| {
                        eprintln!("Error: {:?}", e);
                    });
            return; // Exit the function gracefully instead of panicking
        }
    }; 
    // main loop that keeps the server alive and listening to connections 
    loop {
        // tokio::select! concurrently awaits multiple "async cases" and runs the code for the first one that completes.
        tokio::select! { // select! has await built in so the cases inside the loop execute only after a promise
            // case 1: the listener establishes a connection, this returns Result<> so we handle both the return value and the error
            response = listener.accept() => { 
                match response {
                    Ok((stream, addr)) => {
                        let handle = app_handle.clone(); // we need to clone the handle becasue handle_connection task thread can be spawned every loop so we need a handle for every loop 
                        let shutdown = shutdown_signal.clone(); // clone the shutdown so every task detects it and sends a close frame to its client to also shutdown garcefuly 
                        tokio::spawn(handle_connection(handle, stream, addr, shutdown)); // Spawn a new, separate async task to handle this specific connection. 
                                                  //This allows the main server loop to immediately go back to listening for more connections without being blocked by the new one
                    }
                    Err(e) => { eprintln!("Error: {}", e); }
                } 
                
            }
            // case 2: check if we have a shutdown every 100 milliseconds to avoid 100% CPU usage. if the shutdown is true the server closes
            _ = async {
                loop {
                    if shutdown_signal.load(AtomicOrdering::Relaxed) {
                        break; // break this inner loop
                    }
                    sleep(Duration::from_millis(100)).await;
                }
            } => {
                println!("[WebSocket] Shutdown signal received. Exiting server loop.");
                break; // break the main loop
            }
            
        } 
    } 
}


// handle the stream channel to receive and send data  
async fn handle_connection(app_handle: AppHandle, stream: TcpStream, addr: SocketAddr, shutdown_signal: Arc<AtomicBool>) {
    // establish connection to the stream, this will be the channel where audio data will flow 
    if let Ok(ws_stream) =  accept_async(stream).await {
        // split the stream channel into two parts: a writer (for sending) and a reader (for receiving)
        let (mut write, mut read) = ws_stream.split();

         
        loop {

            tokio::select! {
                biased; // polls the cases in order so this will check the shutdown first


                _ = async { // select! polls hte future retrurned by the shutdown loop, if its 'true' then it executes the promise which is sending close to the client and break the loop

                    loop {
                        if shutdown_signal.load(AtomicOrdering::Relaxed) {
                            break; // break this inner loop
                        }
                        sleep(Duration::from_millis(100)).await;
                    }
                } => {
                    // send a closing message to the client (extension) to signal it that the server is closing
                    let _ = write.send(Message::Close(None)).await;
                    break; // break the main loop
                }


                // this keeps listening to the read stream because it receives update after update so we need to handle one update and awwit the next one with read.next()
                message = read.next() => {
                    match message {
                        Some(Ok(msg)) => {
                            if msg.is_text() || msg.is_binary() {
                                println!("recived message:{:?}", msg);
                                if let Ok(payload) = msg.to_text() {
                                    app_handle.emit("server-message", payload).unwrap_or_else(|e| {
                                        eprintln!("Error: {:?}", e);
                                    });
                                }
                                let send_back = Message::Text("echoing back from tauri".into());
                                if let Err(e) = write.send(send_back).await { // if this is not err then send() will execute 
                                    eprintln!("[WebSocket] Error sending message back to {}: {:?}", addr, e);
                                    break;
                                }
                            }else if msg.is_close() {
                                // a close message was sent from the client
                                println!("[WebSocket] Received close frame from {}. Closing connection.", addr);
                                break;
                            }              
                        }
                        Some(Err(e)) => {
                            // An error occurred while reading from the stream.
                            eprintln!("[WebSocket] Error reading from stream for {}: {:?}", addr, e);
                            break; // Stop the loop on a read error.
                        }
                        None => {
                            break;
                        }
                    }
                }  
                
            }
        }  
        // After the loop, try to properly close the sink
        let _ = write.close().await;

    } else {
        // Handle the error case
        eprintln!("failed to establish connection to {}", addr);
    };
    
}