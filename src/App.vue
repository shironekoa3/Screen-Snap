<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import Sidebar from './components/Sidebar.vue';
import { useCaptureStore } from './stores/capture';
import { useSettingsStore } from './stores/settings';

const captureStore = useCaptureStore();
const settingsStore = useSettingsStore();
const appWindow = getCurrentWindow();
const isMaximized = ref(false);

let unlistenResize: (() => void) | null = null;

async function syncMaximized() {
  isMaximized.value = await appWindow.isMaximized();
}

async function handleMinimize() {
  await appWindow.minimize();
}

async function handleToggleMaximize() {
  await appWindow.toggleMaximize();
  await syncMaximized();
}

async function handleClose() {
  await appWindow.close();
}

onMounted(async () => {
  await settingsStore.load();
  await captureStore.checkStatus();
  await captureStore.loadStorageInfo();
  captureStore.setupListeners();
  await syncMaximized();

  unlistenResize = await appWindow.onResized(() => {
    syncMaximized();
  });
});

onUnmounted(() => {
  captureStore.cleanup();
  if (unlistenResize) unlistenResize();
});
</script>

<template>
  <div
    class="flex flex-col h-screen overflow-hidden bg-slate-50 min-w-0"
  >
    <!-- Custom Title Bar (draggable) -->
    <header
      class="h-9 bg-slate-900 flex items-center justify-between flex-shrink-0 select-none"
      style="-webkit-app-region: drag;"
    >
      <div class="flex items-center gap-2 pl-3">
        <div
          class="w-5 h-5 rounded-md bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center"
        >
          <svg
            class="w-3 h-3 text-white"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z"
            />
          </svg>
        </div>
        <span class="text-white text-xs font-medium">Screen Snap</span>
      </div>

      <!-- Window Control Buttons (no-drag) -->
      <div class="flex items-center h-full" style="-webkit-app-region: no-drag;">
        <button
          class="w-10 h-full flex items-center justify-center text-slate-400 hover:text-white hover:bg-slate-700 transition-colors"
          @click="handleMinimize"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-width="2" d="M5 12h14" />
          </svg>
        </button>
        <button
          class="w-10 h-full flex items-center justify-center text-slate-400 hover:text-white hover:bg-slate-700 transition-colors"
          @click="handleToggleMaximize"
        >
          <svg
            v-if="!isMaximized"
            class="w-3.5 h-3.5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M8 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-2M18 4h-4m4 0v4m0-4L10 14"
            />
          </svg>
          <svg
            v-else
            class="w-3.5 h-3.5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M6 10h2v2H6v-2zm4 0h8v2h-8v-2zm4-6h2v2h-2V4zm0 4h8v2h-8V8zM6 14h2v2H6v-2zm4 0h8v2h-8v-2z"
            />
          </svg>
        </button>
        <button
          class="w-10 h-full flex items-center justify-center text-slate-400 hover:text-white hover:bg-red-500 transition-colors"
          @click="handleClose"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      </div>
    </header>

    <!-- Body: Sidebar + Content -->
    <div class="flex flex-1 overflow-hidden">
      <Sidebar />
      <main class="flex-1 min-w-0 overflow-y-auto overflow-x-hidden">
        <router-view />
      </main>
    </div>
  </div>
</template>
