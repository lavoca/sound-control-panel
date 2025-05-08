<script setup lang="ts">
import { ref, onMounted, Ref } from "vue";
import { invoke } from "@tauri-apps/api/core";


// this will represent the rust types from the back end.
type RustSessionType = [ number, string | null, string, number ] // [PID, UID(string or null), Name, Volume]

// this will be used to map the rust types into vue/typescript freindly ones
type VueSessionType  = {
  pid: number,
  uid: string | null,
  name: string,
  volume: number
}



// this will hold the session data that will be converted from rust type to vue type in order to use it in the template in a vue/typescript freindly way
const sessionData: Ref<VueSessionType[]> = ref([]) // session data is a reactive variable so to annotate it we need Ref<T>, T is the type we want.




async function GetinitialData() {

  sessionData.value = []; // Clear previous data

  // get the session data from backend, this will be an array of tuppels.
  const rustResult = await invoke<RustSessionType[]>("get_sessions_and_volumes");
  // map the back end data to be represented in the front end.
  const vueResult: VueSessionType[] = rustResult.map( item => ({
    pid: item[0],
    uid: item[1],
    name: item[2],
    volume: item[3]
  }))
  
  sessionData.value = vueResult;
  
}




async function ChangeVolume(targetPid: number, targetUid: string | null, setTargetVolume: number) {

  const payload = {
    pid: targetPid,
    uid: targetUid,
    volume: setTargetVolume
  }

  const actualVolume = await invoke<number>("set_volume", payload)

  const sessionIndex = sessionData.value.findIndex(s => s.pid === targetPid && s.uid === targetUid);
    if (sessionIndex !== -1) {
      sessionData.value[sessionIndex].volume = actualVolume;
    }
  
}

onMounted(() => {
  GetinitialData();
});


</script>




<template>
  
  <ul>
    <li v-for="session in sessionData" :key="session.pid" style="margin-bottom: 15px; border-bottom: 1px solid #eee; padding-bottom: 10px;">
    <div>
      PID: {{ session.pid }} | UID: {{ session.uid }} | Name: {{ session.name }} 
    </div>
      <div>
        <input
            type="range"
            min="0"
            max="1"    
            step="0.01"
            :value="session.volume" 
            @input="(event) => {
              // This function is called every time the slider's value changes as the user drags it.
              // 'event' is the DOM input event object.

              // Get the current value from the slider (which is a string)
              // and convert it to a floating-point number.
              // 'event.target' refers to the specific slider input element being interacted with.
              const newFractionValue = parseFloat((event.target as HTMLInputElement).value);

              // Optional: Optimistic UI Update.
              // Immediately update the 'volume' property of the current 'session' object
              // in our local 'sessionData'. This makes the UI feel responsive.
              session.volume = newFractionValue;

              // Call the 'ChangeVolume' function (defined in the <script setup>)
              // to send the update to the Rust backend.
              // 'session.pid' and 'session.uid' identify the specific session.
              // 'newFractionValue' is the new volume to be set.
              ChangeVolume(session.pid, session.uid, newFractionValue);
            }"
            style="width: 200px; vertical-align: middle;"
          />
          <!-- Display volume as percentage -->
          <span style="margin-left: 10px;">{{ (session.volume * 100).toFixed(0) }}%</span>
      </div>
    </li>
  </ul> 
  


  
</template>












