// This file contains the Rust code for managing audio sessions and volumes on Windows using the Windows API.
use std::ffi::OsString; // For handling OS-specific strings
use std::os::windows::ffi::OsStringExt;
// Extension traits for Windows-specific string conversions
use windows::{
    core::{Interface, PWSTR, BOOL}, // Core Windows interface types
    Win32::{Foundation::{CloseHandle, HANDLE}, Media::Audio::{
        eConsole, eRender, Endpoints::IAudioEndpointVolume, IAudioSessionControl, AudioSessionState, AudioSessionStateActive,
        IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionManager2, 
        IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator       // Interface for controlling audio volume
    }, System::{Com::{
        CoCreateInstance, CoTaskMemFree, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED // COM threading model (apartment threaded)
    }, Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS           // Gets next process in a snapshot
    }}},
};
use tauri::{command, State}; // state is used to access the manage store
use crate::audio_monitor::SessionDetails;
use crate::ExtensionData; // wrapper for data that will be sent via tokio mpsc
use tokio::sync::mpsc::Sender;





fn get_process_name_by_id(process_id: u32) -> windows::core::Result<Option<String>> {


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

fn get_session_instance_identifier(session_control2: &IAudioSessionControl2) -> Result<Option<String>, String>{

    unsafe {

        let wide_str_p = session_control2.GetSessionInstanceIdentifier().map_err(|e| format!("Failed to Get the PWSTR with GetSessionInstanceIdentifier: {}", e))?;

        let pwstr_id = if wide_str_p.is_null() {
            None
        }else {
            Some(pwstr_to_string(wide_str_p))
        };

        // Clean up memory (safe even if null)
        CoTaskMemFree(Some(wide_str_p.0 as *const core::ffi::c_void));

        Ok(pwstr_id) 
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

fn pwstr_to_string(pwstr: PWSTR) -> String {

    unsafe{

        // Assume pwstr is not null when this function is called
        let slice = pwstr.as_wide();
        // Convert to OsString and then to String
        let os_string = OsString::from_wide(slice);
        // Convert to regular String, replacing invalid UTF-8 sequences
        os_string.to_string_lossy().into_owned()
    }
}

// gets already runing audio audio sessions (different from audio monitor loop function which only gets new or removed audio sessions)
#[command]
pub async fn get_sessions_and_volumes() -> Result<Vec<SessionDetails>, String> {


    let mut sessions_data = Vec::new();

    unsafe{ CoInitializeEx( None, COINIT_APARTMENTTHREADED ).ok().map_err(|e| format!("COM failed to initialize: {}", e))? };

    let device_enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ).map_err(|e| format!("Failed to create MMDeviceEnumerator instance: {}", e))?
    };

    let default_device = unsafe {
        device_enumerator.GetDefaultAudioEndpoint(
            eRender,
            eConsole,
        ).map_err(|e| format!("Failed to get DefaultAudioEndpoint: {}", e))?
    };

    let session_manager: IAudioSessionManager2 = unsafe {
        default_device.Activate(
            CLSCTX_ALL,
            None,
        ).map_err(|e| format!("Failed to activate IAudioSessionManager2 on default device: {}", e))?
    };

    let session_enumerator: IAudioSessionEnumerator = unsafe { session_manager.GetSessionEnumerator().map_err(|e| format!("Failed to get SessionEnumerator: {}", e))? };
    let session_count = unsafe { session_enumerator.GetCount().map_err(|e| format!("Failed to get session count: {}", e))? };

    for i in 0..session_count {

        let session_control: IAudioSessionControl = unsafe { session_enumerator.GetSession(i).map_err(|e| format!("Failed to Get Session i: {}", e))?};
        let session_volume_control: ISimpleAudioVolume = session_control.cast().map_err(|e| format!("Failed to cast ISimpleAudioVolume on session_control: {}", e))?;
        let session_current_volume = unsafe{ session_volume_control.GetMasterVolume().map_err(|e| format!("Failed to Get session MasterVolume: {}", e))? };
        let muted = unsafe{ session_volume_control.GetMute().map_err(|e| format!("Failed to Get session mute state: {}", e))? };
        #[allow(non_upper_case_globals)]
        let active = unsafe {session_control.GetState()}.map(|state| {
            match state {
                AudioSessionStateActive => true,
                _ => false, // here we only really need the AudioSessionStateInactive to be false but since we need to exhaust every other possible arm we group them all to be false 
            }
        }).map_err(|e| format!("Failed to Get session active state: {}", e))?; 
        let session_control2: IAudioSessionControl2 = session_control.cast().map_err(|e| format!("Failed to cast IAudioSessionControl2 on session_control: {}", e))?;

        let uid = unsafe {
            let pwstr = session_control2.GetSessionInstanceIdentifier().map_err(|e| format!("Failed to Get session active state: {}", e))?;
            match take_pwstr_to_string(pwstr) { // this converts and frees the pwstr
                Some(uid) => uid,
                None => {
                    eprintln!("[AudioMonitor] Could not get a valid UID for a session. Skipping it.");
                    return Err(format!(
                        "Session has no instance identifier. HRESULT: {:?}",
                        windows::Win32::Foundation::E_FAIL));
                }
            }
        };

        let process_id = unsafe { session_control2.GetProcessId().unwrap_or(0) };
        if process_id == 0 {
            continue;
        }

        let display_name: String  = unsafe {
            let pwstr = session_control2.GetDisplayName().map_err(|e| format!("Failed to Get session active state: {}", e))?;
            take_pwstr_to_string(pwstr)
        }
        .or_else(|| get_process_name_by_id(process_id).ok() // here if pwstr is none or string is empty we use this function to get another name
        .flatten())
        .unwrap_or_else(|| "unknown".to_string());
        
        sessions_data.push(SessionDetails{
            process_id: process_id, 
            session_uid: uid, 
            process_name: display_name, 
            session_volume: session_current_volume, 
            is_muted: muted.as_bool(), 
            is_active: active
        });

        //sessions_data.push((process_id, uid, display_name, session_current_volume, muted.as_bool(), active));
    }

    unsafe { CoUninitialize(); }
    Ok(sessions_data)

    
}
    

#[command]
pub async fn set_volume (pid: u32, uid: String, volume: f32) -> Result<(), String> {

    println!("RUST: set_volume CALLED! PID: {}, Target Volume: {}", pid, volume); // <<< THIS IS CRITICAL


    //let mut current_volume: f32 = 0.0;

    unsafe{ CoInitializeEx( None, COINIT_APARTMENTTHREADED ).ok().map_err(|e| format!("COM failed to initialize: {}", e))? };

    let device_enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ).map_err(|e| format!("Failed to create MMDeviceEnumerator instance: {}", e))?
    };

    let default_device = unsafe {
        device_enumerator.GetDefaultAudioEndpoint(
            eRender,
            eConsole,
        ).map_err(|e| format!("Failed to get DefaultAudioEndpoint: {}", e))?
    };

    let session_manager: IAudioSessionManager2 = unsafe {
        default_device.Activate(
            CLSCTX_ALL,
            None,
        ).map_err(|e| format!("Failed to activate IAudioSessionManager2 on default device: {}", e))?
    };

    let session_enumerator: IAudioSessionEnumerator = unsafe { session_manager.GetSessionEnumerator().map_err(|e| format!("Failed to get SessionEnumerator: {}", e))? };
    let session_count = unsafe { session_enumerator.GetCount().map_err(|e| format!("Failed to get session count: {}", e))? };

    for i in 0..session_count {

        let session_control: IAudioSessionControl = unsafe { 
            match session_enumerator.GetSession(i) {
                Ok(control) => control,
                Err(e) => {
                    eprintln!("Failed to Get Session {}: {:?}", i, e);
                    continue; // if one session fails we skip it to the next session in the loop
                }
            }
        };


        let session_control2: IAudioSessionControl2 = match session_control.cast() {
            Ok(control2) => control2,
                Err(e) => {
                eprintln!("Failed to cast Session {}: {:?}", i, e);
                continue; // if one session fails we skip it to the next session in the loop
            }
        };


        let session_identifier =  match get_session_instance_identifier(&session_control2) {
            Ok(Some(uid)) => if uid.is_empty() {
                eprintln!("session {} does not have a uid, skipping it.", i);
                continue;
            }else {uid}
            Ok(None) => {
                eprintln!("failed to find a uid for session {}, skipping it.", i);
                continue;
            }
            Err(_) => {
                eprintln!("Error occurred while trying to find a uid for session {}, skipping it.", i);
                continue;
            }
        };

        let process_id = unsafe { session_control2.GetProcessId().unwrap_or(0) };
        if process_id == 0 {
            eprintln!("Invalid process ID for session {}", i);
            continue;
        }

        if pid == process_id && uid == session_identifier {
            let simple_audio_volume: ISimpleAudioVolume = session_control.cast().map_err(|e| format!("Failed to cast ISimpleAudioVolume on session_control: {}", e))?;
            unsafe {
                simple_audio_volume.SetMasterVolume(
                    volume, // the app volume is supposed to be a percentage of the master volume so windows will multiplly this to the master volume of windows
                    std::ptr::null()
                ).map_err(|e| format!("Failed to SetMasterVolume on {}: {:?}", i, e))?;
                //current_volume = simple_audio_volume.GetMasterVolume().map_err(|e| format!("Failed to GetMasterVolume: {}", e))?;   
                 
            }
        }
        
    }
    unsafe { CoUninitialize(); }
    Ok(())

}




#[command]
pub async fn set_mute(pid: u32, uid: String, mute: bool) -> Result<(), String> {
    
    unsafe{ CoInitializeEx( None, COINIT_APARTMENTTHREADED ).ok().map_err(|e| format!("COM failed to initialize: {}", e))? };

    let device_enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ).map_err(|e| format!("Failed to create MMDeviceEnumerator instance: {}", e))?
    };

    let default_device = unsafe {
        device_enumerator.GetDefaultAudioEndpoint(
            eRender,
            eConsole,
        ).map_err(|e| format!("Failed to get DefaultAudioEndpoint: {}", e))?
    };

    let session_manager: IAudioSessionManager2 = unsafe {
        default_device.Activate(
            CLSCTX_ALL,
            None,
        ).map_err(|e| format!("Failed to activate IAudioSessionManager2 on default device: {}", e))?
    };

    let session_enumerator: IAudioSessionEnumerator = unsafe { session_manager.GetSessionEnumerator().map_err(|e| format!("Failed to get SessionEnumerator: {}", e))? };
    let session_count = unsafe { session_enumerator.GetCount().map_err(|e| format!("Failed to get session count: {}", e))? };

    for i in 0..session_count {

        let session_control: IAudioSessionControl = unsafe { 
            match session_enumerator.GetSession(i) {
                Ok(control) => control,
                Err(e) => {
                    eprintln!("Failed to Get Session {}: {:?}", i, e);
                    continue; // if one session fails we skip it to the next session in the loop
                }
            }
        };


        let session_control2: IAudioSessionControl2 = match session_control.cast() {
            Ok(control2) => control2,
                Err(e) => {
                eprintln!("Failed to cast Session {}: {:?}", i, e);
                continue; // if one session fails we skip it to the next session in the loop
            }
        };


        let session_identifier =  match get_session_instance_identifier(&session_control2) {
            Ok(Some(uid)) => if uid.is_empty() {
                eprintln!("session {} does not have a uid, skipping it.", i);
                continue;
            }else {uid}
            Ok(None) => {
                eprintln!("failed to find a uid for session {}, skipping it.", i);
                continue;
            }
            Err(_) => {
                eprintln!("Error occurred while trying to find a uid for session {}, skipping it.", i);
                continue;
            }
        };

        let process_id = unsafe { session_control2.GetProcessId().unwrap_or(0) };
        if process_id == 0 {
            eprintln!("Invalid process ID for session {}", i);
            continue;
        }

        if pid == process_id && uid == session_identifier {
            let simple_audio_volume: ISimpleAudioVolume = session_control.cast().map_err(|e| format!("Failed to cast ISimpleAudioVolume on session_control: {}", e))?;
            unsafe {
                let is_mute = simple_audio_volume.GetMute().map_err(|e| format!("Failed to GetMute on {}: {:?}", i, e))?;
                if is_mute.as_bool() != mute {
                    simple_audio_volume.SetMute(
                        mute, 
                        std::ptr::null()
                    ).map_err(|e| format!("Failed to SetMute on {}: {:?}", i, e))?;
                
                    
                }    
            }
        
        }
        
    }
    unsafe { CoUninitialize(); }
    Ok(())


}

// invoked from frontend and sneds the tab volume data to the websocket server using a tokio mpsc channel created in 'setup()'
// injected with 'command_sender' a tokio mpsc sender from the tauri manage store to send that data wrapped in a ExtensionData type
#[command]
pub async fn set_tab_volume (tab_id: u32, volume: f64, command_sender: State<'_, Sender<ExtensionData>>) -> Result<(), String> {

    let volume_command = ExtensionData::SetVolume { tab_id, volume };

    command_sender.send(volume_command).await.map_err(|e| e.to_string())?;
    Ok(()) 
  
}
#[command]
pub async fn set_tab_mute(tab_id: u32, mute: bool, initial_volume: f64, command_sender: State<'_, Sender<ExtensionData>>) -> Result<(), String> {
    
    let mute_command = ExtensionData::SetMute { tab_id, mute, initial_volume };

    command_sender.send(mute_command).await.map_err(|e| e.to_string())?;
    Ok(())
}