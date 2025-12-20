// This line prevents a console window from appearing on Windows in release builds.
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use tauri::{Manager, WindowEvent}; // Manager is needed for app.manage() and state()

mod commands;
mod audio_monitor;



#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // 1. Get an AppHandle. This is a controller for the app that is safe to
            //    send to other threads. We'll pass this to our monitor thread so it
            //    can send events to the frontend.
            let monitor_app_handle = app.handle().clone(); // for the monitor_thread_loop
            let websocket_app_handle = app.handle().clone(); // for the websocket_server

            // 2. Create the shared shutdown signal.
            //    This is an Arc<AtomicBool> that both the main app and the monitor thread
            //    will have access to. It starts as `false`.
            let shutdown_flag = Arc::new(AtomicBool::new(false));

            // 3. Store the shutdown signal in Tauri's managed state.
            //    This allows us to retrieve it later in the `on_window_event` handler
            //    when the user tries to close the window.
            // The `manage` method makes `Arc<AtomicBool>` available throughout the app.
            app.manage(shutdown_flag.clone());// here we are creating a new Arc pointer that points to the exact same AtomicBool on the heap, we are not cloning atomicbool itself
            // any part of the application that has access to an AppHandle or a Window object can now retrieve this shared state


            let monitor_thread_signal = shutdown_flag.clone(); // clone the shutdown arc to give it to the monitor thread
            // 4. Spawn the dedicated background thread for audio monitoring.
            //    `std::thread::spawn` starts a new OS thread.
            //    The `move` keyword gives the closure ownership of the variables it uses
            //    (app_handle and shutdown_flag).
            std::thread::spawn(move || {
                // The new thread will execute this function from our audio_monitor module.
                // the params are app_handle for communicating with the frontend and shutdown_flag for graceful termination.
                audio_monitor::monitor_thread_loop(monitor_app_handle, monitor_thread_signal);
            });
            // --- End of the Audio Monitor Setup ---

            // spawn a tokio non blocking task to handle the websocket server that listens to audio updates from the browser extension
            // No need for a clone pf the shutdown_flag if this is the last use of `shutdown_flag` in `setup`
            tauri::async_runtime::spawn(audio_monitor::websocket_server(websocket_app_handle, shutdown_flag));

            // The setup hook must return Ok(()) to indicate success.
            Ok(())
        })
        // --- WINDOW EVENT HOOK: This listens for events on all windows ---
        .on_window_event(|window, event| match event {
            // Listen for the user trying to close a window.
            WindowEvent::CloseRequested { .. } => {

                // Retrieve the shutdown signal from Tauri's managed state.
                if let Some(shutdown_flag) = window.try_state::<Arc<AtomicBool>>() {
                    // Atomically set the flag to `true`.
                    // The monitor thread's loop will see this change and break.
                    shutdown_flag.store(true, Ordering::Relaxed);

                    // Optional: Give the monitor thread a moment to run its cleanup code
                    // (unregistering listeners, CoUninitialize). This can help prevent
                    // abrupt termination issues. 500ms is usually more than enough.
                    //std::thread::sleep(std::time::Duration::from_millis(500));
                } else {
                    eprintln!("[Tauri] Could not get shutdown_flag from state to signal monitor thread.");
                }
            }
            _ => {} // Ignore all other window events.
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![commands::audio::set_volume, commands::audio::set_mute, commands::audio::get_sessions_and_volumes])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
