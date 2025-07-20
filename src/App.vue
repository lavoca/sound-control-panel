<script setup lang="ts">
import { ref, onMounted, onUnmounted, Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, Event } from "@tauri-apps/api/event";


// this will represent the session details from the backend
type SessionData  = {
  pid: number,
  uid: string,
  name: string,
  volume: number,
  is_muted: boolean,
  is_active: boolean,
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



// this will hold the session data that will be converted from rust type to vue type in order to use it in the template in a vue/typescript freindly way
const sessionData: Ref<SessionData[]> = ref([]) // sessionData is a reactive variable so to annotate it we need Ref<T>, T is the type we want.


// This is the setup for the cleanup logic. We declare variables that will
// eventually hold the functions needed to stop listening to events.
// The type `(() => void) | null` means "a function that takes no arguments and returns nothing, OR null".
// This is the exact signature of the function that `listen()` returns.
let unlistenGetData: (() => void) | null = null;
let unlistenVolumeChanged: (() => void) | null = null;
let unlistenClosed: (() => void) | null = null;
let unlistenStateChanged: (() => void) | null = null;
let unlistenCriticalError: (() => void) | null = null;


// this function to scan for already running audio sessions and is called when onMounted cycle.
async function GetInitialData() {
  const initialSessions = await invoke<SessionData[]>("get_sessions_and_volumes");
  sessionData.value = initialSessions;
}

// this function runs on Mounted to get initial session data to populate the ui
// the back end loops over existing sessions and sends one session data for every itteration
// so this function that listens to the backend event gets called for every detected session 
function GetSessionData(event: Event<SessionData>) {

  console.log("RECEIVED EVENT: 'audio-session-created'", event);
  // we check if a session is already in the session list brfore pushing the session's data into it
  const sessionIndex = sessionData.value.findIndex(s => s.pid === event.payload.pid && s.uid === event.payload.uid);
  if (sessionIndex === -1) { // if we dont find the session (-1 means false)
    sessionData.value.push(event.payload) // push methode signals vue to rerender the ui
  }else {
    sessionData.value = sessionData.value.map(session => {
      return session.uid === event.payload.uid ? event.payload : session;
    })
  }
  
};


// this function invokes the back end and changes a specific session's volume but doesnt get back the confirmation that the volume changed to the frontend 
// instead we optimisticaly change the volume in the ui to before the backend so it feels responsive and instaniouss 
// the front end instead will know about the change and confirm it through the CheckVolumeChanged function that gets an event from the backend when it detects the change
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
  invoke<number>("set_volume", payload)
  
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
      return {...session, is_muted: payload.mute}
    }else {
      return session;
    }
  });
  
  invoke<boolean>("set_mute", payload)

}

// this function detects changes to the volume of a session and the mute state by a given uid
// this is where we will get back the volume value that ChangeVolume function did set in the back end and also the mute state that ToggleMute function did set
function CheckVolumeChanged(event: Event<VolumeChangedPayload>) {
  console.log("RECEIVED EVENT: 'audio-session-volume-changed', Payload:", event.payload);
  
  sessionData.value = sessionData.value.map(session => {
    if(session.uid === event.payload.uid) {
      return {...session, volume: event.payload.newVolume ,is_muted: event.payload.isMuted};
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



onMounted(async () => {

  await GetInitialData();
  unlistenGetData = await listen<SessionData>("audio-session-created", GetSessionData);
  unlistenVolumeChanged = await listen<VolumeChangedPayload>("audio-session-volume-changed", CheckVolumeChanged);
  unlistenStateChanged = await listen<SessionStatePayload>("session-state-changed", SessionState);
  unlistenClosed = await listen<string>("audio-session-closed", SessionClosed);

});

onUnmounted(() => {
  if(unlistenGetData) unlistenGetData();
  if(unlistenVolumeChanged) unlistenVolumeChanged();
  if(unlistenStateChanged) unlistenStateChanged();
  if(unlistenClosed) unlistenClosed();
});


</script>




<template>
  <!-- Main container with padding and spacing for list items -->
  <div class="p-4 space-y-4">
    
    <!-- A helpful message if the list is empty -->
    <div v-if="sessionData.length === 0" class="text-center text-gray-400">
      No active audio sessions found. Play some audio to see it here.
    </div>

    <!-- The v-for loop to render each session -->
    <!-- We use `session.uid` as the key because it's guaranteed to be unique -->
    <div
      v-for="session in sessionData"
      :key="session.uid"
      class="flex items-center justify-between p-3 bg-gray-700 rounded-lg shadow-md transition-opacity duration-300"
      :class="{ 'opacity-50': !session.is_active }"
    >
      <!-- Session Info (Name and PID) -->
      <div class="flex flex-col">
        <!-- Make sure the property names here EXACTLY match your TypeScript `SessionData` type -->
        <span class="font-bold text-white">{{ session.name }}</span>
        <span class="text-xs text-gray-400">PID: {{ session.pid }}</span>
      </div>

      <!-- Volume Controls -->
      <div class="flex items-center space-x-4">
        <!-- Volume Slider -->
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          :value="session.volume"
          @input="ChangeVolume(session.pid, session.uid, ($event.target as HTMLInputElement).valueAsNumber)"
          class="w-40 h-2 bg-gray-500 rounded-lg appearance-none cursor-pointer"
        />
        
        <!-- Volume Percentage -->
        <span class="w-12 text-sm text-center text-gray-300">{{ (session.volume * 100).toFixed(0) }}%</span>

        <!-- Mute Button -->
        <button
          @click="ToggleMute(session.pid, session.uid, !session.is_muted)"
          class="px-4 py-1 text-sm font-semibold text-white rounded-md transition-colors duration-200"
          :class="session.is_muted ? 'bg-red-600 hover:bg-red-700' : 'bg-blue-600 hover:bg-blue-700'"
        >
          {{ session.is_muted ? 'Unmute' : 'Mute' }}
        </button>
      </div>
    </div>
  </div>
</template>















