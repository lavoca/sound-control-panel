use std::io;
use std::ffi::OsString; // For handling OS-specific strings
use std::os::windows::ffi::OsStringExt; // Extension traits for Windows-specific string conversions
use std::path::Path; // For handling file paths
use windows::{
    core::{Interface, PWSTR}, // Core Windows interface types
    Win32::{Foundation::{CloseHandle, HANDLE}, Media::Audio::{
        eConsole, eRender, Endpoints::IAudioEndpointVolume, IAudioSessionControl, IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator       // Interface for controlling audio volume
    }, System::{Com::{
        CoCreateInstance, CoTaskMemFree, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED // COM threading model (apartment threaded)
    }, Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32First, Module32Next, Process32FirstW, Process32NextW, MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPPROCESS           // Gets next process in a snapshot
    }, ProcessStatus::{
        K32EnumProcessModules,    // Enumerates modules in a process
        K32GetModuleBaseNameW,    // Gets base name of a module
    }, Threading::{
        OpenProcess,              // Opens a handle to a process
        PROCESS_QUERY_INFORMATION, // Process access right for querying info
        PROCESS_VM_READ,          // Process access right for reading memory
    }}},
};
use tauri::command;





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


#[command]
pub async fn get_sessions_and_volumes() -> Result<Vec<(u32, Option<String>, String, f32)>, String> {

    let mut sessions_data: Vec<( u32, Option<String>, String, f32)> = Vec::new();

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

        let session_control: IAudioSessionControl = unsafe { session_enumerator.GetSession(i).map_err(|e| format!("Failed to Get Session: {}", e))?};
        let session_volume_control: ISimpleAudioVolume = session_control.cast().map_err(|e| format!("Failed to cast ISimpleAudioVolume on session_control: {}", e))?;
        let session_current_volume = unsafe{ session_volume_control.GetMasterVolume().map_err(|e| format!("Failed to Get session MasterVolume: {}", e))? };
        let session_control2: IAudioSessionControl2 = session_control.cast().map_err(|e| format!("Failed to cast IAudioSessionControl2 on session_control: {}", e))?;

        let session_identifier =  get_session_instance_identifier(&session_control2)?; 

        let process_id = unsafe { session_control2.GetProcessId().unwrap_or(0) };
        if process_id == 0 {
            continue;
        }

        match get_process_name_by_id(process_id).map_err(|e| format!("Failed to match process id to name: {}", e))? {
            Some(process_name) => {
                
                sessions_data.push((process_id, session_identifier, process_name, session_current_volume));
                  
            }

            None => {
                eprintln!("Session {}: Process ID: {} (name unknown)", i, process_id);
            }
        }

    }

    unsafe { CoUninitialize(); }
    Ok(sessions_data)



    
}

#[command]
pub async fn set_volume (pid: u32, uid: Option<String>, volume: f32) -> Result<f32, String> {

    println!("RUST: set_volume CALLED! PID: {}, Target Volume: {}", pid, volume); // <<< THIS IS CRITICAL


    let mut current_volume: f32 = 0.0;

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
        let session_control: IAudioSessionControl = unsafe { session_enumerator.GetSession(i).map_err(|e| format!("Failed to Get Session: {}", e))?};
        let session_control2: IAudioSessionControl2 = session_control.cast().map_err(|e| format!("Failed to cast IAudioSessionControl2 on session_control: {}", e))?;
        let session_identifier =  get_session_instance_identifier(&session_control2)?;

        let process_id = unsafe { session_control2.GetProcessId().unwrap_or(0) };
        if process_id == 0 {
            continue;
        }

        if pid == process_id || uid == session_identifier {
            let simple_audio_volume: ISimpleAudioVolume = session_control.cast().map_err(|e| format!("Failed to cast ISimpleAudioVolume on session_control: {}", e))?;
            unsafe {
                simple_audio_volume.SetMasterVolume(
                    volume, // the app volume is supposed to be a percentage of the master volume so windows will multiplly this to the master volume of windows
                    std::ptr::null()
                ).map_err(|e| format!("Failed to SetMasterVolume on: {}", e))?;
                current_volume = simple_audio_volume.GetMasterVolume().map_err(|e| format!("Failed to GetMasterVolume: {}", e))?;    
            }
        }
    }
    Ok(current_volume)

}