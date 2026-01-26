# Sound Control Panel

A powerful, real-time audio mixer for Windows that provides granular control over both system applications and individual browser tabs. Built with a high-performance Rust backend, a modern Vue 3 frontend using Tauri, and a dedicated browser extension.

> [!WARNING]
> For per-tab audio control, the companion [Browser Extension](https://github.com/lavoca/audio-control-extension/releases/latest) must also be installed. 

![Application Screenshot](public/screenshots/app_recording.gif)

---

## Installation

1.  Go to the **[Latest Release](https://github.com/lavoca/sound-control-panel/releases/latest)** page.
2.  Download and run the `.msi` installer for the desktop application.
3.  Download and install the `audio-control-extension.zip` from the same release page by following the instructions in the [extension's README](https://github.com/lavoca/audio-control-extension/releases/latest).

### Core Features

*   **Unified Audio Control:** Manage audio from both native desktop applications and individual browser tabs in one place.
*   **Live Session Discovery:** Automatically detects and displays applications and browser tabs as they start or stop playing audio—no refresh needed.
*   **Per-App & Per-Tab Control:** Provides individual volume sliders and mute controls for each audio source.
*   **Real-Time Sync:** UI updates instantly in response to system events and changes made in the browser.
*   **Lightweight & Performant:** Built with Rust for a minimal resource footprint and a responsive user experience.


### Architecture Overview

This project is a complete, end-to-end system demonstrating bi-directional, real-time communication between a native desktop application, a browser extension, and the operating system itself.

![Application Architecture Diagram](public/screenshots/diagram.svg)

*   **Tauri Backend (Rust):** A multi-threaded Rust application serves as the core.
    *   An **Audio Monitor** thread interfaces directly with Windows Core Audio (COM) APIs to listen for system-level audio events.
    *   An asynchronous **WebSocket Server** (built with Tokio) manages real-time communication with the browser extension.
    *   **Tauri Commands** handle requests from the UI, sending messages to the WebSocket server via a thread-safe MPSC channel.
*   **Browser Extension (TypeScript):**
    *   A **Content Script** injects into web pages to detect and control audio/video elements.
    *   A **Background Script** acts as the central hub, managing a persistent WebSocket connection to the Tauri app and aggregating audio state from all tabs.
*   **Frontend UI (Vue 3):**
    *   A reactive UI built with Vue 3 and TypeScript.
    *   Receives live state updates (new sessions, volume changes, tab lists) pushed from the Rust backend via Tauri's event system.

    
### Tech Stack

*   **Desktop App Backend:** Rust, Tauri, Tokio
*   **OS Interop:** `windows-rs` for Windows Core Audio (COM) APIs
*   **Concurrency:** `std::thread`, `Arc`, `AtomicBool`, `tokio::sync::{mpsc, broadcast}`
*   **Frontend:** Vue 3, TypeScript, Tailwind CSS
*   **Browser Extension:** TypeScript, WebExtensions API
*   **Communication Protocol:** WebSockets

### Prerequisites

*   [Rust](https://www.rust-lang.org/tools/install)
*   [Node.js & pnpm](https://pnpm.io/installation)
*   Tauri development prerequisites (see the [Tauri guide](https://tauri.app/start/prerequisites/))

### Running in Development Mode

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/lavoca/sound-control-panel.git
    cd sound-control-panel
    ```

2.  **Install frontend dependencies:**
    ```bash
    pnpm install
    ```

3.  **Run the Tauri development server:**
    ```bash
    pnpm tauri dev
    ```

4.  **Clone and run the browser extension:**
    *   In a separate terminal, clone the [audio-control-extension](https://github.com/lavoca/audio-control-extension) repository.
    *   Follow the instructions in its README to run it in development mode (e.g., `pnpm install` then `pnpm dev`).
    *   Load the unpacked extension from its output directory (e.g., `.output/chrome-mv3`) into your browser.

    
---

This project is licensed under the **MIT License**.