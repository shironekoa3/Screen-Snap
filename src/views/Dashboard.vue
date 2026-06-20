<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { useCaptureStore } from '../stores/capture';
import { useSettingsStore } from '../stores/settings';

const captureStore = useCaptureStore();
const settingsStore = useSettingsStore();

const isStarting = ref(false);
const isStopping = ref(false);
const isCapturing = ref(false);
const showToast = ref(false);
const toastMessage = ref('');

function showToastMsg(msg: string) {
  toastMessage.value = msg;
  showToast.value = true;
  setTimeout(() => {
    showToast.value = false;
  }, 2500);
}

async function handleStart() {
  isStarting.value = true;
  try {
    await settingsStore.save();
    await captureStore.start(
      settingsStore.intervalSecs,
      settingsStore.format,
      settingsStore.quality
    );
    showToastMsg('开始定时截图');
  } catch (e: any) {
    showToastMsg('启动失败: ' + (e?.toString() || '未知错误'));
  } finally {
    isStarting.value = false;
  }
}

async function handleStop() {
  isStopping.value = true;
  try {
    await captureStore.stop();
    showToastMsg('已停止截图');
  } catch (e: any) {
    showToastMsg('停止失败: ' + (e?.toString() || '未知错误'));
  } finally {
    isStopping.value = false;
  }
}

async function handleManualCapture() {
  isCapturing.value = true;
  try {
    await captureStore.takeScreenshot(
      settingsStore.format,
      settingsStore.quality
    );
    showToastMsg('截图已保存');
  } catch (e: any) {
    showToastMsg('截图失败: ' + (e?.toString() || '未知错误'));
  } finally {
    isCapturing.value = false;
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

const lastPreviewUrl = computed(() => {
  return captureStore.lastScreenshotDataUrl;
});

const intervalPresets = [
  { label: '5 秒', value: 5 },
  { label: '10 秒', value: 10 },
  { label: '30 秒', value: 30 },
  { label: '1 分钟', value: 60 },
  { label: '5 分钟', value: 300 },
  { label: '10 分钟', value: 600 },
  { label: '30 分钟', value: 1800 },
  { label: '1 小时', value: 3600 },
];

onMounted(async () => {
  await captureStore.loadStorageInfo();
});
</script>

<template>
  <div class="p-4 lg:p-8 max-w-6xl mx-auto">
    <!-- Page header -->
    <div class="flex items-center justify-between mb-8">
      <div>
        <h2 class="text-2xl font-bold text-slate-800">控制台</h2>
        <p class="text-slate-500 text-sm mt-0.5">管理屏幕截图录制与查看状态</p>
      </div>
      <button
        class="btn-ghost text-xs"
        @click="captureStore.loadStorageInfo()"
        title="刷新数据"
      >
        <svg
          class="w-4 h-4"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
          />
        </svg>
        刷新
      </button>
    </div>

    <!-- Stats cards -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
      <!-- Status -->
      <div class="card p-5">
        <div class="flex items-center gap-3 mb-3">
          <div
            class="w-9 h-9 rounded-lg flex items-center justify-center"
            :class="captureStore.isRunning ? 'bg-emerald-50' : 'bg-slate-100'"
          >
            <span class="relative flex h-2.5 w-2.5">
              <span
                v-if="captureStore.isRunning"
                class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"
              ></span>
              <span
                class="relative inline-flex rounded-full h-2.5 w-2.5"
                :class="captureStore.isRunning ? 'bg-emerald-500' : 'bg-slate-400'"
              ></span>
            </span>
          </div>
          <span class="text-xs font-medium text-slate-500 uppercase tracking-wide"
            >状态</span
          >
        </div>
        <p class="text-2xl font-bold text-slate-800">
          {{ captureStore.isRunning ? '录制中' : '已停止' }}
        </p>
        <p class="text-xs text-slate-400 mt-1">
          {{ captureStore.isRunning ? '定时截图进行中' : '点击下方按钮开始' }}
        </p>
      </div>

      <!-- Today captures -->
      <div class="card p-5">
        <div class="flex items-center gap-3 mb-3">
          <div class="w-9 h-9 rounded-lg bg-indigo-50 flex items-center justify-center">
            <svg
              class="w-5 h-5 text-indigo-500"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
              />
            </svg>
          </div>
          <span class="text-xs font-medium text-slate-500 uppercase tracking-wide"
            >今日截图</span
          >
        </div>
        <p class="text-2xl font-bold text-slate-800">
          {{ captureStore.todayCount }}
        </p>
        <p class="text-xs text-slate-400 mt-1">
          {{ formatBytes(captureStore.todaySizeBytes) }}
        </p>
      </div>

      <!-- Total captures -->
      <div class="card p-5">
        <div class="flex items-center gap-3 mb-3">
          <div class="w-9 h-9 rounded-lg bg-blue-50 flex items-center justify-center">
            <svg
              class="w-5 h-5 text-blue-500"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4"
              />
            </svg>
          </div>
          <span class="text-xs font-medium text-slate-500 uppercase tracking-wide"
            >总计截图</span
          >
        </div>
        <p class="text-2xl font-bold text-slate-800">
          {{ captureStore.totalCount }}
        </p>
        <p class="text-xs text-slate-400 mt-1">
          {{ formatBytes(captureStore.totalSizeBytes) }}
        </p>
      </div>

      <!-- Disk usage -->
      <div class="card p-5">
        <div class="flex items-center gap-3 mb-3">
          <div class="w-9 h-9 rounded-lg bg-amber-50 flex items-center justify-center">
            <svg
              class="w-5 h-5 text-amber-500"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M3 15a4 4 0 004 4h9a5 5 0 10-.1-9.999 5.002 5.002 0 10-9.78 2.096A4.001 4.001 0 003 15z"
              />
            </svg>
          </div>
          <span class="text-xs font-medium text-slate-500 uppercase tracking-wide"
            >存储占用</span
          >
        </div>
        <p class="text-2xl font-bold text-slate-800">
          {{ formatBytes(captureStore.totalSizeBytes) }}
        </p>
        <p class="text-xs text-slate-400 mt-1">
          截图存储于 AppData 目录
        </p>
      </div>
    </div>

    <!-- Main content: Controls + Preview -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- Capture Controls -->
      <div class="card p-6 col-span-1">
        <h3 class="text-base font-semibold text-slate-800 mb-5">截图控制</h3>

        <!-- Start / Stop -->
        <div class="mb-6">
          <button
            v-if="!captureStore.isRunning"
            class="btn-primary w-full py-3 text-base"
            :disabled="isStarting"
            @click="handleStart"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"
              />
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
            {{ isStarting ? '启动中...' : '开始定时截图' }}
          </button>
          <button
            v-else
            class="btn-danger w-full py-3 text-base"
            :disabled="isStopping"
            @click="handleStop"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
              />
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M9 10a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z"
              />
            </svg>
            {{ isStopping ? '停止中...' : '停止截图' }}
          </button>
        </div>

        <!-- Manual capture -->
        <button
          class="btn-ghost w-full mb-6 border border-slate-200 hover:border-slate-300 py-2.5"
          :disabled="isCapturing"
          @click="handleManualCapture"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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
          {{ isCapturing ? '截图中...' : '立即截图' }}
        </button>

        <!-- Separator -->
        <div class="border-t border-slate-100 my-5"></div>

        <!-- Interval -->
        <div class="mb-4">
          <label class="block text-xs font-medium text-slate-500 mb-2"
            >截图间隔</label
          >
          <select
            class="select-field"
            :value="settingsStore.intervalSecs"
            :disabled="captureStore.isRunning"
            @change="
              settingsStore.intervalSecs = Number(
                ($event.target as HTMLSelectElement).value
              )
            "
          >
            <option
              v-for="p in intervalPresets"
              :key="p.value"
              :value="p.value"
            >
              {{ p.label }}
            </option>
          </select>
        </div>

        <!-- Format -->
        <div class="mb-4">
          <label class="block text-xs font-medium text-slate-500 mb-2"
            >图片格式</label
          >
          <div class="flex gap-3">
            <label
              class="flex-1 flex items-center justify-center gap-2 px-3 py-2.5 rounded-lg border cursor-pointer transition-all duration-150"
              :class="
                settingsStore.format === 'png'
                  ? 'border-indigo-500 bg-indigo-50 text-indigo-700'
                  : 'border-slate-200 text-slate-600 hover:border-slate-300'
              "
            >
              <input
                type="radio"
                value="png"
                v-model="settingsStore.format"
                :disabled="captureStore.isRunning"
                class="sr-only"
              />
              <span class="text-sm font-medium">PNG</span>
              <span class="text-xs text-slate-400">无损</span>
            </label>
            <label
              class="flex-1 flex items-center justify-center gap-2 px-3 py-2.5 rounded-lg border cursor-pointer transition-all duration-150"
              :class="
                settingsStore.format === 'jpg'
                  ? 'border-indigo-500 bg-indigo-50 text-indigo-700'
                  : 'border-slate-200 text-slate-600 hover:border-slate-300'
              "
            >
              <input
                type="radio"
                value="jpg"
                v-model="settingsStore.format"
                :disabled="captureStore.isRunning"
                class="sr-only"
              />
              <span class="text-sm font-medium">JPG</span>
              <span class="text-xs text-slate-400">压缩</span>
            </label>
          </div>
        </div>

        <!-- Quality (JPG only) -->
        <div v-if="settingsStore.format === 'jpg'" class="mb-2">
          <label class="block text-xs font-medium text-slate-500 mb-2">
            压缩质量: <span class="text-indigo-600 font-semibold">{{ settingsStore.quality }}%</span>
          </label>
          <input
            type="range"
            min="10"
            max="100"
            step="5"
            :value="settingsStore.quality"
            :disabled="captureStore.isRunning"
            @input="
              settingsStore.quality = Number(
                ($event.target as HTMLInputElement).value
              )
            "
            class="w-full h-2 bg-slate-200 rounded-lg appearance-none cursor-pointer accent-indigo-500"
          />
          <div class="flex justify-between text-xs text-slate-400 mt-1">
            <span>小文件</span>
            <span>高质量</span>
          </div>
        </div>
      </div>

      <!-- Recent Capture Preview -->
      <div class="card p-6 col-span-2">
        <h3 class="text-base font-semibold text-slate-800 mb-4">最近截图</h3>

        <div
          v-if="lastPreviewUrl"
          class="relative rounded-lg overflow-hidden bg-slate-100"
        >
          <img
            :src="lastPreviewUrl"
            :key="lastPreviewUrl"
            alt="最新截图"
            class="w-full h-auto max-h-[440px] object-contain"
          />
        </div>

        <div
          v-else
          class="flex flex-col items-center justify-center py-16 text-slate-400"
        >
          <svg
            class="w-16 h-16 mb-4 text-slate-300"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="1.5"
              d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z"
            />
          </svg>
          <p class="text-sm">暂无截图</p>
          <p class="text-xs mt-1">点击「立即截图」或「开始定时截图」</p>
        </div>

        <!-- Error display -->
        <div
          v-if="captureStore.error"
          class="mt-4 p-3 rounded-lg bg-red-50 border border-red-100 text-sm text-red-600"
        >
          {{ captureStore.error }}
        </div>
      </div>
    </div>

    <!-- Toast notification -->
    <transition name="toast">
      <div
        v-if="showToast"
        class="fixed bottom-6 right-6 px-5 py-3 bg-slate-800 text-white text-sm rounded-lg shadow-lg z-50"
      >
        {{ toastMessage }}
      </div>
    </transition>
  </div>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(12px);
}

/* Range input styling */
input[type='range']::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #6366f1;
  cursor: pointer;
  border: 2px solid white;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}
</style>
