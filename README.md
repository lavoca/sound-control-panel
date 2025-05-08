# Sound Control Panel

This project is a desktop application built with [Tauri](https://tauri.app/), [Vue 3](https://vuejs.org/), and [TypeScript](https://www.typescriptlang.org/). It allows users to view and control the audio sessions and their volumes on their system.

## Features

- View all active audio sessions with their process names and unique identifiers.
- Adjust the volume of individual audio sessions in real-time.
- Built with a modern stack: Tauri for the backend, Vue 3 for the frontend, and Rust for system-level operations.

## Setup Instructions

1. **Install Dependencies**:
   - Install [Node.js](https://nodejs.org/) and [Rust](https://www.rust-lang.org/).
   - Run `npm install` in the project directory to install the required Node.js dependencies.

2. **Development**:
   - Run `npm run dev` to start the development server and Tauri application.

3. **Build**:
   - Run `npm run build` to build the frontend.
   - Run `npm run tauri build` to package the application.

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) with:
  - [Volar](https://marketplace.visualstudio.com/items?itemName=Vue.volar) for Vue 3 support.
  - [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) for Tauri integration.
  - [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) for Rust development.

## License

This project is licensed under the MIT License.
