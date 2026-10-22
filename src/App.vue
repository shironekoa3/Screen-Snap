<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import Sidebar from './components/Sidebar.vue';
import appIcon from './assets/app-icon.png';
import { useCaptureStore } from './stores/capture';
import { useSettingsStore } from './stores/settings';

const captureStore = useCaptureStore();
const settingsStore = useSettingsStore();
const appWindow = getCurrentWindow();
const isMaximized = ref(false);
const trayHint = ref(false);

let unlistenResize: (() => void) | null = null;
let unlistenClose: UnlistenFn | null = null;
let unlistenFocus: UnlistenFn | null = null;
let hintTimer: ReturnType<typeof setTimeout> | null = null;
/** 刚被收进托盘、还没提示过 —— 等下次窗口重新出现时再提示 */
let hintPending = false;

const TRAY_HINT_KEY = 'screen-snap-tray-hint-shown';

/**
 * 后端已经把窗口收进托盘了。
 * 注意：隐藏动作由后端立刻完成，前端不再做延时隐藏，
 * 否则用户重新打开窗口后会被那个延时任务再关一次。
 */
function handleHiddenToTray() {
  if (localStorage.getItem(TRAY_HINT_KEY) === '1') return;
  hintPending = true;
}

/** 窗口重新获得焦点时，把「已收进托盘」的提示补上 */
function maybeShowTrayHint() {
  if (!hintPending) return;
  hintPending = false;
  localStorage.setItem(TRAY_HINT_KEY, '1');
  trayHint.value = true;
  if (hintTimer) clearTimeout(hintTimer);
  hintTimer = setTimeout(() => {
    trayHint.value = false;
    hintTimer = null;
  }, 2600);
}

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
  await captureStore.loadLastScreenshot();
  captureStore.setupListeners();
  await syncMaximized();

  unlistenResize = await appWindow.onResized(() => {
    syncMaximized();
  });

  unlistenClose = await listen('hidden-to-tray', handleHiddenToTray);

  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    if (focused) maybeShowTrayHint();
  });
});

onUnmounted(() => {
  captureStore.cleanup();
  if (unlistenResize) unlistenResize();
  if (unlistenClose) unlistenClose();
  if (unlistenFocus) unlistenFocus();
  if (hintTimer) clearTimeout(hintTimer);
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
        <img
          :src="appIcon"
          alt="Screen Snap"
          class="w-5 h-5 rounded-full select-none"
          draggable="false"
        />
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
          :title="settingsStore.closeToTray ? '收进托盘' : '退出程序'"
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
        <router-view v-slot="{ Component }">
          <keep-alive>
            <component :is="Component" />
          </keep-alive>
        </router-view>
      </main>
    </div>

    <!-- 关闭到托盘的提示 -->
    <transition name="fade">
      <div
        v-if="trayHint"
        class="fixed bottom-6 left-1/2 -translate-x-1/2 px-4 py-2.5 bg-slate-800/95 text-white text-xs rounded-lg shadow-lg z-50 pointer-events-none"
      >
        已收进托盘，程序仍在后台运行 · 右键托盘图标可退出
      </div>
    </transition>
  </div>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
