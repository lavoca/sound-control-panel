<script setup lang="ts">
import { ref, onMounted, onUnmounted, Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, Event } from "@tauri-apps/api/event";
import { throttle } from "lodash-es";

// the current page being shown it will be either processes audio or tabs audio
const currentView = ref<'processes' | 'tabs'>('processes');

// this will represent the session details from the backend
type SessionData  = {
  pid: number,
  uid: string,
  name: string,
  volume: number,
  isMuted: boolean,
  is_active: boolean,
}
// represents a tab's audio from the browser extension
type AudioTab = {
  tabId: number;
  tabUrl: string;
  tabTitle: string;
  isAudible: boolean;
  hasContentAudio: boolean;
  isMuted: boolean;
  paused: boolean;
  volume: number;
  lastUpdate: number;
}


type VolumeChangedPayload = {
  uid: String,
  newVolume: number,
  isMuted: boolean
}

type SessionStatePayload = {
  uid: string,
  is_active: boolean,
}

// map to hold all the starting slider volumes for every tab
const startVolumes = new Map<number, number>();
// this will hold the session data that will be converted from rust type to vue type in order to use it in the template in a vue/typescript freindly way
const sessionData: Ref<SessionData[]> = ref([]); // sessionData is a reactive variable so to annotate it we need Ref<T>, T is the type we want.
// holds audio tabs from the extension to use in the ui
const audioTabsData: Ref<AudioTab[]> = ref([]);


// This is the setup for the cleanup logic. We declare variables that will
// eventually hold the functions needed to stop listening to events.
// The type `(() => void) | null` means "a function that takes no arguments and returns nothing, OR null".
// This is the exact signature of the function that `listen()` returns.
let unlistenGetData: (() => void) | null = null;
let unlistenVolumeChanged: (() => void) | null = null;
let unlistenClosed: (() => void) | null = null;
let unlistenStateChanged: (() => void) | null = null;
let unlistenAudioTabs: (() => void) | null = null;


// this function to scan for already running audio sessions and is called in onMounted cycle.
async function GetInitialData() {
  // invokes the #[command] functions in the backend
  const initialSessions = await invoke<SessionData[]>("get_sessions_and_volumes"); // the invoke type should match the command function return type
  sessionData.value = initialSessions;
}



// this function runs on Mounted to get initial session data to populate the ui
// the backend loops over existing sessions and sends one session data for every itteration
// so this function that listens to the backend event gets called for every detected session 
function GetSessionData(event: Event<SessionData>) { // the event is just one SessionData object that we will push into an array of the same type
  // listens to app_handle.emit() events and we register this function to listen to the events in onMounted hook
  console.log("RECEIVED EVENT: 'audio-session-created'", event);
  // we check if a session is already in the session list brfore pushing the session's data into it
  const sessionIndex = sessionData.value.findIndex(s => s.pid === event.payload.pid && s.uid === event.payload.uid);
  if (sessionIndex === -1) { // if we dont find the session (-1 means false)
    sessionData.value.push(event.payload) // push methode signals vue to rerender the ui
  }else {
    sessionData.value[sessionIndex] = event.payload; // replace the old value with the new one 
  }
};



// this function invokes the back end and changes a specific session's volume but doesnt get back the confirmation that the volume changed to the frontend 
// instead we optimisticaly change the volume in the ui to before the backend so it feels responsive and instaniouss 
// the front end instead will know about the change and confirm it through the 'CheckVolumeChanged' function that gets an event from the backend when it detects the change
function ChangeVolume(pid: number, uid: string, setTargetVolume: number) { // prameters's values from template
  const payload = { 
    pid: pid,
    uid: uid,
    volume: setTargetVolume,
    
  }
 // update the sessiondata's volume from template before we invoke and set the volume in backend
 // use .map to replace the old sessionDetails lsit with a new updated one so that vue rerenders the ui
  sessionData.value = sessionData.value.map(session => {
    if(session.uid === payload.uid) {
      return {...session, volume: payload.volume};
    }else {
      return session;
    }
  })
  // set volume in backend
  invoke<void>("set_volume", payload);
  
}

// simillar to the ChangeVolume this function also invokes the backend to set a mute state but does not get back the confirmation and leaves that to the CheckVolumeChanged function
// it sets is_mute to either true or false
function ToggleMute(pid: number, uid: string, isMute: boolean) {

  const payload = {
    pid: pid,
    uid: uid,
    mute: isMute,
  }
  // we also optimisticaly change the mute state in the ui to before the backend so it feels responsive
  sessionData.value = sessionData.value.map(session => {
    if(session.uid === payload.uid) {
      return {...session, isMuted: payload.mute}
    }else {
      return session;
    }
  });
  
  invoke<void>("set_mute", payload);

}

// this function detects changes to the volume of a session and the mute state by a given uid
// this is where we will get back the volume value that ChangeVolume function did set in the back end and also the mute state that ToggleMute function did set
function CheckVolumeChanged(event: Event<VolumeChangedPayload>) {
  console.log("RECEIVED EVENT: 'audio-session-volume-changed', Payload:", event.payload);
  
  sessionData.value = sessionData.value.map(session => {
    if(session.uid === event.payload.uid) {
      return {...session, volume: event.payload.newVolume ,isMuted: event.payload.isMuted};
    }else {
      return session;
    }
  });
}

// this will listen for closed or expired audio sessions and gets their uid from backend and removes them from the session list sessionData
function SessionClosed(event: Event<string>) {
  console.log("RECEIVED EVENT: 'audio-session-closed', Payload:", event.payload);
  const sessionIndex = sessionData.value.findIndex(s => s.uid === event.payload);
  if (sessionIndex !== -1) {
    sessionData.value.splice(sessionIndex, 1)
  }
}

// this will listen for the change in state of a session and return if its active or not by setting is_active to true or false
function SessionState(event: Event<SessionStatePayload>) {
  console.log("RECEIVED EVENT: 'session-state-changed', Payload:", event.payload);
 sessionData.value = sessionData.value.map(session => {
  if(session.uid === event.payload.uid) {
    return {...session, is_active: event.payload.is_active};
  }else {
    return session;
  }
 })
}


// listens to backend websocket server for audio tabs from the extension
function GetExtensionAudioTabs(event: Event<AudioTab[]>) { // the event is an array of AudioTab objects
  console.log("RECEIVED EVENT: 'audio-tabs-received'", event);
  audioTabsData.value = event.payload; // replace the whole array from event with the array 'audioTabsData' evry time we get updates

} // we could make the backend send just one 'AudioTab' object at a time and populate the array 'audioTabsData' with them but just for demonstration that sending a list/array can also work



// this function sends tab volumes to a 'tauri command function' with 'invoke' 
// the command function wraps the received volume value ands sends it through a tokio 'mpsc channel' to the websocket server in audio_monnitor  
// the websocket server receives the volume and sends it back to the Extension so it can apply the new volume 
function _ChangeTabVolume(tabId: number, volume: number) {
  const payload = {
    tabId: tabId,
    volume: volume,
  };
  invoke<void>('set_tab_volume', payload);
}

const ChangeTabVolume = throttle(_ChangeTabVolume, 50, {leading: true, trailing: true});

// captures the value when the slider first gets pressed this value will serve as a returning point when we unmute from volume = 0
function captureStartVolume(tabID:number, initialVolume: number) {
  startVolumes.set(tabID, initialVolume); // push the volume and the tabID associated with it as the key
}

function ToggleTabMute(tabId: number, isMuted: boolean) {
  let startVolume = startVolumes.get(tabId);
  const payload = {
    tabId: tabId,
    mute: isMuted,
    initialVolume: startVolume,
  };
  invoke<void>('set_tab_mute', payload);
}

onMounted(async () => {

  await GetInitialData(); // this one is an invoke function it does not listen so we dont need to free a listener
  unlistenGetData = await listen<SessionData>("audio-session-created", GetSessionData);
  unlistenVolumeChanged = await listen<VolumeChangedPayload>("audio-session-volume-changed", CheckVolumeChanged);
  unlistenStateChanged = await listen<SessionStatePayload>("session-state-changed", SessionState);
  unlistenClosed = await listen<string>("audio-session-closed", SessionClosed);
  unlistenAudioTabs = await listen<AudioTab[]>("extension-audio-tabs", GetExtensionAudioTabs);

});

onUnmounted(() => {
  if(unlistenGetData) unlistenGetData();
  if(unlistenVolumeChanged) unlistenVolumeChanged();
  if(unlistenStateChanged) unlistenStateChanged();
  if(unlistenClosed) unlistenClosed();
  if(unlistenAudioTabs) unlistenAudioTabs();
});


</script>




<template>
  <!-- Main container with a dark background, padding, and spacing for list items -->
  <!-- NEW: Added overflow-hidden to contain the sliding animation -->
  <div class="bg-gray-900 text-gray-200 min-h-screen p-4 font-sans overflow-hidden">
    <h1 class="text-2xl font-bold text-center mb-6 text-white">Audio Control</h1>
    
    <!-- NEW: View Toggle Buttons -->
    <div class="flex justify-center mb-6 bg-gray-800 p-1 rounded-full w-max mx-auto">
      <button
        @click="currentView = 'processes'"
        class="px-6 py-2 text-sm font-semibold rounded-full transition-colors duration-300"
        :class="currentView === 'processes' ? 'bg-blue-600 text-white' : 'text-gray-400 hover:bg-gray-700'"
      >
        Processes
      </button>
      <button
        @click="currentView = 'tabs'"
        class="px-6 py-2 text-sm font-semibold rounded-full transition-colors duration-300"
        :class="currentView === 'tabs' ? 'bg-blue-600 text-white' : 'text-gray-400 hover:bg-gray-700'"
      >
        Browser Tabs
      </button>
    </div>

    <!-- NEW: Page container to hold both sliding views -->
    <div class="relative w-full h-auto">

      <!-- NEW: Transition wrapper for the Processes View -->
      <Transition name="slide-fade">
        <!-- RENDER PROCESSES VIEW -->
        <!-- NEW: Changed v-if to v-show to work better with transitions -->
        <div v-show="currentView === 'processes'" class="w-full">
          <!-- A helpful message if the list is empty, styled for the dark theme -->
          <div v-if="sessionData.length === 0" class="text-center text-gray-500 py-10">
            <p>No active audio sessions found.</p>
            <p class="text-sm">Play some audio to see it here.</p>
          </div>

          <!-- The list container -->
          <div class="space-y-3">
            <!-- The v-for loop to render each session -->
            <div
              v-for="session in sessionData"
              :key="session.uid"
              class="
                flex items-center justify-between p-4
                bg-gray-800/50 backdrop-blur-sm border border-gray-700/50 
                rounded-xl shadow-lg transition-all duration-300 hover:bg-gray-700/60
              "
              :class="{ 'opacity-60': !session.is_active }"
            >
              <!-- Session Info (Name and PID) -->
              <div class="flex flex-col">
                <span class="font-semibold text-white text-lg">{{ session.name }}</span>
                <span class="text-xs text-gray-400">PID: {{ session.pid }}</span>
              </div>

              <!-- Volume Controls -->
              <div class="flex items-center space-x-4">
                <!-- Volume Slider -->
                <!-- Custom classes are needed for styling the slider's track and thumb -->
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.01"
                  :value="session.volume"
                  @input="ChangeVolume(session.pid, session.uid, ($event.target as HTMLInputElement).valueAsNumber)"
                  class="volume-slider w-48"
                />
                
                <!-- Volume Percentage -->
                <span class="w-12 text-sm text-center text-gray-400 font-mono">{{ (session.volume * 100).toFixed(0) }}%</span>

                <!-- Mute Button -->
                <button
                  @click="ToggleMute(session.pid, session.uid, !session.isMuted)"
                  class="
                    w-20 px-4 py-2 text-sm font-semibold text-white rounded-full 
                    transition-all duration-200 ease-in-out
                    focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-gray-900
                  "
                  :class="session.isMuted 
                    ? 'bg-red-600 hover:bg-red-700 focus:ring-red-500' 
                    : 'bg-gray-600 hover:bg-gray-500 focus:ring-blue-500'"
                >
                  {{ session.isMuted ? 'Unmute' : 'Mute' }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </Transition>

      <!-- NEW: Transition wrapper for the Browser Tabs View -->
      <Transition name="slide-fade">
        <!-- RENDER BROWSER TABS VIEW -->
        <!-- NEW: This whole block is new. It's positioned on top of the other view and toggled with v-show -->
        <div v-show="currentView === 'tabs'" class="w-full absolute top-0 left-0">
          <!-- A helpful message if the tabs list is empty -->
          <div v-if="audioTabsData.length === 0" class="text-center text-gray-500 py-10">
            <p>No browser tabs with audio are currently detected.</p>
          </div>
          <!-- The list container for tabs -->
          <div class="space-y-3">
            <!-- The v-for loop to render each tab -->
            <div
              v-for="tab in audioTabsData"
              :key="tab.tabId"
              class="
                flex items-center justify-between p-4
                bg-gray-800/50 backdrop-blur-sm border border-gray-700/50 
                rounded-xl shadow-lg transition-all duration-300 hover:bg-gray-700/60
              "
              :class="{ 'opacity-60': !tab.isAudible }"
            >
              <!-- Tab Info (Title and URL) -->
              <div class="flex flex-col w-1/3">
                <span class="font-semibold text-white text-lg truncate" :title="tab.tabTitle">{{ tab.tabTitle }}</span>
                <span class="text-xs text-gray-400 truncate" :title="tab.tabUrl">{{ tab.tabUrl }}</span>
              </div>

              <!-- Volume Controls for Tabs -->
              <div class="flex items-center space-x-4">
                <!-- Volume Slider for Tabs -->
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.01"
                  :value="tab.volume"
                  @mousedown="captureStartVolume(tab.tabId, tab.volume)"
                  @input="ChangeTabVolume(tab.tabId, ($event.target as HTMLInputElement).valueAsNumber)"
                  class="volume-slider w-48"
                />
                
                <!-- Volume Percentage for Tabs -->
                <span class="w-12 text-sm text-center text-gray-400 font-mono">{{ (tab.volume * 100).toFixed(0) }}%</span>

                <!-- Mute Button for Tabs -->
                <button
                  @click="ToggleTabMute(tab.tabId, !tab.isMuted)"
                  class="
                    w-20 px-4 py-2 text-sm font-semibold text-white rounded-full 
                    transition-all duration-200 ease-in-out
                    focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-offset-gray-900
                  "
                  :class="tab.isMuted 
                    ? 'bg-red-600 hover:bg-red-700 focus:ring-red-500' 
                    : 'bg-gray-600 hover:bg-gray-500 focus:ring-blue-500'"
                >
                  {{ tab.isMuted ? 'Unmute' : 'Mute' }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style>

/* We define some CSS variables that match our Tailwind color scheme for consistency */
:root {
  --slider-track-bg: #4a5568; /* gray-600 */
  --slider-thumb-bg: #e2e8f0; /* gray-200 */
  --slider-thumb-border: #4299e1; /* blue-400 */
}

.volume-slider {
  /* Resetting the default browser styles is crucial */
  -webkit-appearance: none;
  appearance: none;
  background: transparent; /* The track background is handled by pseudo-elements */
  cursor: pointer;
}

/* --- Track Styling --- */
.volume-slider::-webkit-slider-runnable-track {
  background-color: var(--slider-track-bg);
  height: 0.35rem;
  border-radius: 0.5rem;
}
.volume-slider::-moz-range-track {
  background-color: var(--slider-track-bg);
  height: 0.35rem;
  border-radius: 0.5rem;
}

/* --- Thumb (the draggable circle) Styling --- */
.volume-slider::-webkit-slider-thumb {
  -webkit-appearance: none; /* Override default look */
  appearance: none;
  margin-top: -6px; /* Center the thumb on the track */
  
  background-color: var(--slider-thumb-bg);
  height: 1.25rem; /* 20px */
  width: 1.25rem; /* 20px */
  border-radius: 9999px; /* Make it a perfect circle */
  border: 3px solid var(--slider-thumb-border);
  transition: background-color 0.2s ease-in-out;
}
.volume-slider::-moz-range-thumb {
  background-color: var(--slider-thumb-bg);
  height: 1.25rem;
  width: 1.25rem;
  border-radius: 9999px;
  border: 3px solid var(--slider-thumb-border);
  transition: background-color 0.2s ease-in-out;
}

/* Add a hover effect for the thumb */
.volume-slider:hover::-webkit-slider-thumb {
  background-color: #ffffff;
}
.volume-slider:hover::-moz-range-thumb {
  background-color: #ffffff;
}

/* --- NEW: Transition Styles for the Page Slide --- */
.slide-fade-enter-active {
  transition: all 0.4s ease-out;
}
.slide-fade-leave-active {
  transition: all 0.4s ease-in;
  /* Position absolute is needed on the leaving element to prevent layout shifts */
  position: absolute;
}
.slide-fade-enter-from {
  opacity: 0;
  transform: translateX(30px);
}
.slide-fade-leave-to {
  opacity: 0;
  transform: translateX(-30px);
}

</style>













