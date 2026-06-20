<script setup lang="ts">
import { useCaptureStore } from '../stores/capture';
import { useRoute } from 'vue-router';

const captureStore = useCaptureStore();
const route = useRoute();

const navItems = [
  { path: '/', label: '控制台', icon: 'dashboard' },
  { path: '/history', label: '历史记录', icon: 'history' },
];
</script>

<template>
  <aside
    class="w-60 bg-slate-900 flex flex-col select-none flex-shrink-0 overflow-y-auto"
  >
    <!-- Logo / Brand -->
    <div class="px-5 pt-4 pb-5">
      <div class="flex items-center gap-3">
        <div
          class="w-9 h-9 rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center shadow-lg shadow-indigo-500/25"
        >
          <svg
            class="w-5 h-5 text-white"
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
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M15 13a3 3 0 11-6 0 3 3 0 016 0z"
            />
          </svg>
        </div>
        <div>
          <h1 class="text-white font-semibold text-sm leading-tight">
            Screen Snap
          </h1>
          <p class="text-slate-400 text-xs">屏幕快照记录</p>
        </div>
      </div>
    </div>

    <!-- Navigation -->
    <nav class="flex-1 px-3 py-2 space-y-1 border-t border-slate-700/60">
      <router-link
        v-for="item in navItems"
        :key="item.path"
        :to="item.path"
        class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-150"
        :class="
          route.path === item.path
            ? 'bg-indigo-500/15 text-indigo-400'
            : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800'
        "
      >
        <!-- Dashboard icon -->
        <svg
          v-if="item.icon === 'dashboard'"
          class="w-5 h-5 flex-shrink-0"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z"
          />
        </svg>
        <!-- History icon -->
        <svg
          v-if="item.icon === 'history'"
          class="w-5 h-5 flex-shrink-0"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        {{ item.label }}
      </router-link>
    </nav>

    <!-- Bottom status -->
    <div class="px-4 py-3.5 border-t border-slate-700/60">
      <div class="flex items-center gap-2.5">
        <span class="relative flex h-2.5 w-2.5">
          <span
            v-if="captureStore.isRunning"
            class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"
          ></span>
          <span
            class="relative inline-flex rounded-full h-2.5 w-2.5"
            :class="captureStore.isRunning ? 'bg-emerald-400' : 'bg-slate-500'"
          ></span>
        </span>
        <span class="text-slate-400 text-xs font-medium">
          {{ captureStore.isRunning ? '正在录制' : '已停止' }}
        </span>
      </div>
    </div>
  </aside>
</template>
