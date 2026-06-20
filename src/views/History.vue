<script setup lang="ts">
import { ref, reactive, onMounted, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';

const dates = ref<string[]>([]);
const selectedDate = ref<string>('');
const screenshots = ref<string[]>([]);
const loadingDates = ref(true);
const loadingImages = ref(false);
const lightboxImage = ref<string | null>(null);
const lightboxPath = ref<string>('');
const lightboxOcrText = ref<string>('');
const imageCache = reactive<Record<string, string>>({});
const loadingSet = new Set<string>();

function getMimeType(path: string): string {
  return path.toLowerCase().endsWith('.jpg') || path.toLowerCase().endsWith('.jpeg')
    ? 'image/jpeg'
    : 'image/png';
}

function getCached(filepath: string): string | undefined {
  return imageCache[filepath];
}

async function getImageUrl(filepath: string): Promise<string> {
  // Return cached data URL if already loaded
  const cached = imageCache[filepath];
  if (cached) return cached;

  // Deduplicate concurrent requests for the same file
  if (loadingSet.has(filepath)) {
    await new Promise((r) => setTimeout(r, 100));
    if (imageCache[filepath]) return imageCache[filepath];
  }

  loadingSet.add(filepath);
  try {
    const base64 = await invoke<string>('read_screenshot_data', { filepath });
    const dataUrl = `data:${getMimeType(filepath)};base64,${base64}`;
    imageCache[filepath] = dataUrl; // reactive — triggers template updates
    return dataUrl;
  } catch (e) {
    console.error('读取截图数据失败:', e);
    return '';
  } finally {
    loadingSet.delete(filepath);
  }
}

async function prefetchImages() {
  // Load all images with limited concurrency (4 at a time)
  const concurrency = 4;
  for (let i = 0; i < screenshots.value.length; i += concurrency) {
    const batch = screenshots.value.slice(i, i + concurrency);
    await Promise.allSettled(batch.map(getImageUrl));
  }
}

async function loadDates() {
  loadingDates.value = true;
  try {
    dates.value = await invoke<string[]>('get_dates_with_screenshots');
    if (dates.value.length > 0 && !selectedDate.value) {
      selectedDate.value = dates.value[0];
    }
  } catch (e) {
    console.error('加载日期失败:', e);
  } finally {
    loadingDates.value = false;
  }
}

async function loadScreenshots() {
  if (!selectedDate.value) {
    screenshots.value = [];
    return;
  }
  loadingImages.value = true;
  // Clear reactive cache
  Object.keys(imageCache).forEach((k) => delete imageCache[k]);
  loadingSet.clear();
  try {
    const files = await invoke<string[]>('get_screenshots_by_date', {
      date: selectedDate.value,
    });
    screenshots.value = files;
    // Prefetch in background
    prefetchImages();
  } catch (e) {
    console.error('加载截图列表失败:', e);
    screenshots.value = [];
  } finally {
    loadingImages.value = false;
  }
}

async function openLightbox(filepath: string) {
  lightboxPath.value = filepath;
  lightboxImage.value = null;
  lightboxOcrText.value = '';
  // Fetch image and OCR text in parallel
  const [imageUrl, ocrText] = await Promise.all([
    getImageUrl(filepath),
    invoke<string | null>('get_ocr_text', { filepath }).catch(() => null),
  ]);
  lightboxImage.value = imageUrl;
  lightboxOcrText.value = ocrText || '';
}

function closeLightbox() {
  lightboxImage.value = null;
  lightboxPath.value = '';
  lightboxOcrText.value = '';
}

async function deleteImage(filepath: string) {
  try {
    await invoke('delete_screenshot', { filepath });
    screenshots.value = screenshots.value.filter((f) => f !== filepath);
    if (lightboxPath.value === filepath) {
      closeLightbox();
    }
    // Reload dates in case the folder is now empty
    loadDates();
  } catch (e) {
    console.error('删除失败:', e);
  }
}

function formatDate(dateStr: string): string {
  try {
    // Parse as local time to avoid timezone offset skew
    const d = new Date(dateStr + 'T00:00:00');
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const diffMs = today.getTime() - d.getTime();
    const diffDays = Math.round(diffMs / (1000 * 60 * 60 * 24));

    if (diffDays === 0) return '今天';
    if (diffDays === 1) return '昨天';
    if (diffDays < 7) return `${diffDays} 天前`;

    return d.toLocaleDateString('zh-CN', {
      month: 'long',
      day: 'numeric',
      weekday: 'short',
    });
  } catch {
    return dateStr;
  }
}

function getFilename(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

function getTimeFromPath(path: string): string {
  const filename = getFilename(path);
  // filename like "14-30-25.png" or "14-30-25.jpg"
  const name = filename.replace(/\.(png|jpg|jpeg)$/i, '');
  return name;
}

// Handle keyboard shortcuts in lightbox
function onKeydown(e: KeyboardEvent) {
  if (!lightboxImage.value) return;
  if (e.key === 'Escape') {
    closeLightbox();
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    const idx = screenshots.value.indexOf(lightboxPath.value);
    if (e.key === 'ArrowLeft' && idx > 0) {
      openLightbox(screenshots.value[idx - 1]);
    } else if (e.key === 'ArrowRight' && idx < screenshots.value.length - 1) {
      openLightbox(screenshots.value[idx + 1]);
    }
  }
}

watch(selectedDate, () => {
  loadScreenshots();
});

onMounted(() => {
  loadDates().then(() => {
    if (selectedDate.value) loadScreenshots();
  });
  document.addEventListener('keydown', onKeydown);
});
</script>

<template>
  <div class="p-4 lg:p-8 max-w-7xl mx-auto min-w-0">
    <!-- Page header -->
    <div class="flex items-center justify-between mb-8">
      <div>
        <h2 class="text-2xl font-bold text-slate-800">历史记录</h2>
        <p class="text-slate-500 text-sm mt-0.5">按日期浏览已保存的屏幕截图</p>
      </div>
      <button class="btn-ghost text-xs" @click="loadDates(); loadScreenshots()">
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

    <!-- Empty state -->
    <div
      v-if="!loadingDates && dates.length === 0"
      class="card p-16 flex flex-col items-center justify-center text-center"
    >
      <svg
        class="w-20 h-20 text-slate-300 mb-5"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="1.5"
          d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4"
        />
      </svg>
      <h3 class="text-lg font-semibold text-slate-600 mb-2">暂无历史截图</h3>
      <p class="text-sm text-slate-400 max-w-md">
        返回控制台开始定时截图或手动截图，截图将按日期存储在此处供您查看。
      </p>
    </div>

    <!-- Content -->
    <div v-else class="flex flex-col lg:flex-row gap-4 lg:gap-6">
      <!-- Date sidebar -->
      <div class="w-full lg:w-56 flex-shrink-0">
        <div class="card p-4">
          <h3 class="text-xs font-semibold text-slate-400 uppercase tracking-wider mb-3 px-1">
            日期列表
          </h3>

          <div v-if="loadingDates" class="space-y-2">
            <div
              v-for="i in 3"
              :key="i"
              class="h-9 bg-slate-100 rounded-lg animate-pulse"
            ></div>
          </div>

          <div v-else class="flex lg:block gap-2 lg:gap-0 lg:space-y-1 lg:max-h-[560px] overflow-x-auto lg:overflow-x-hidden overflow-y-hidden lg:overflow-y-auto pr-1">
            <button
              v-for="date in dates"
              :key="date"
              @click="selectedDate = date"
              class="lg:w-full text-left px-3 py-2.5 rounded-lg text-sm transition-all duration-150 flex-shrink-0"
              :class="
                selectedDate === date
                  ? 'bg-indigo-50 text-indigo-700 font-medium'
                  : 'text-slate-600 hover:bg-slate-50'
              "
            >
              <div>{{ formatDate(date) }}</div>
              <div class="text-xs mt-0.5 opacity-60">{{ date }}</div>
            </button>
          </div>
        </div>
      </div>

      <!-- Image grid -->
      <div class="flex-1 min-w-0">
        <div class="card p-5 min-h-[500px]">
          <!-- Selected date header -->
          <div class="flex items-center justify-between mb-4">
            <h3 class="text-sm font-semibold text-slate-600">
              {{ selectedDate ? `${selectedDate} 的截图` : '请选择日期' }}
              <span
                v-if="!loadingImages && screenshots.length > 0"
                class="text-slate-400 font-normal ml-2"
              >
                · {{ screenshots.length }} 张
              </span>
            </h3>
          </div>

          <!-- Loading -->
          <div v-if="loadingImages" class="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <div
              v-for="i in 8"
              :key="i"
              class="aspect-video bg-slate-100 rounded-lg animate-pulse"
            ></div>
          </div>

          <!-- Empty for selected date -->
          <div
            v-else-if="screenshots.length === 0"
            class="flex flex-col items-center justify-center py-16 text-slate-400"
          >
            <svg
              class="w-12 h-12 mb-3 text-slate-300"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="1.5"
                d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z"
              />
            </svg>
            <p class="text-sm">该日期暂无截图</p>
          </div>

          <!-- Grid -->
          <div v-else class="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <div
              v-for="filepath in screenshots"
              :key="filepath"
              class="group relative aspect-video rounded-lg overflow-hidden bg-slate-100 cursor-pointer border-2 border-transparent hover:border-indigo-400 transition-all duration-150 shadow-sm hover:shadow-md"
              @click="openLightbox(filepath)"
            >
              <!-- Thumbnail -->
              <img
                v-if="imageCache[filepath]"
                :src="imageCache[filepath]"
                :alt="getFilename(filepath)"
                loading="lazy"
                class="w-full h-full object-cover transition-transform duration-200 group-hover:scale-105"
              />

              <!-- Loading placeholder -->
              <div
                v-if="!imageCache[filepath]"
                class="absolute inset-0 bg-slate-100 animate-pulse flex items-center justify-center"
              >
                <svg
                  class="w-6 h-6 text-slate-300 animate-spin"
                  fill="none"
                  viewBox="0 0 24 24"
                >
                  <circle
                    class="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="4"
                  ></circle>
                  <path
                    class="opacity-75"
                    fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
                  ></path>
                </svg>
              </div>

              <!-- Overlay with time and delete -->
              <div
                class="absolute inset-0 bg-gradient-to-t from-black/60 via-transparent to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-200 flex flex-col justify-end p-2.5"
              >
                <div class="flex items-center justify-between">
                  <span class="text-white text-xs font-medium">
                    {{ getTimeFromPath(filepath) }}
                  </span>
                  <button
                    class="p-1.5 rounded-md bg-white/20 hover:bg-red-500 text-white transition-colors duration-150"
                    title="删除"
                    @click.stop="deleteImage(filepath)"
                  >
                    <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        stroke-width="2"
                        d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                      />
                    </svg>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Lightbox modal -->
    <Teleport to="body">
      <div
        v-if="lightboxImage"
        class="fixed inset-0 z-50 bg-black/85 flex items-center justify-center"
        @click.self="closeLightbox"
      >
        <!-- Close button -->
        <button
          class="absolute top-5 right-5 p-2.5 rounded-lg bg-white/10 hover:bg-white/20 text-white transition-colors z-10"
          @click="closeLightbox"
        >
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>

        <!-- Nav buttons -->
        <button
          class="absolute left-5 top-1/2 -translate-y-1/2 p-2.5 rounded-lg bg-white/10 hover:bg-white/20 text-white transition-colors"
          @click.stop="
            () => {
              const idx = screenshots.indexOf(lightboxPath);
              if (idx > 0) openLightbox(screenshots[idx - 1]);
            }
          "
        >
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>

        <button
          class="absolute right-5 top-1/2 -translate-y-1/2 p-2.5 rounded-lg bg-white/10 hover:bg-white/20 text-white transition-colors"
          @click.stop="
            () => {
              const idx = screenshots.indexOf(lightboxPath);
              if (idx < screenshots.length - 1) openLightbox(screenshots[idx + 1]);
            }
          "
        >
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>

        <!-- Content: image + OCR text side by side on large screens -->
        <div class="flex flex-col lg:flex-row items-center gap-4 max-w-[95vw] max-h-[90vh]">
          <!-- Image -->
          <img
            :src="lightboxImage"
            alt="预览"
            class="max-w-full lg:max-w-[70vw] max-h-[70vh] object-contain rounded-lg shadow-2xl"
          />

          <!-- OCR text panel -->
          <div
            v-if="lightboxOcrText"
            class="w-full lg:w-72 max-h-[70vh] overflow-y-auto bg-white/5 backdrop-blur-sm rounded-lg p-4 border border-white/10"
          >
            <h4 class="text-white/70 text-xs font-semibold uppercase tracking-wider mb-3">
              识别文字
            </h4>
            <p class="text-white/90 text-sm leading-relaxed whitespace-pre-wrap">
              {{ lightboxOcrText }}
            </p>
          </div>
        </div>

        <!-- Bottom info bar -->
        <div class="absolute bottom-5 left-1/2 -translate-x-1/2 flex items-center gap-4 px-5 py-2.5 bg-white/10 backdrop-blur-sm rounded-lg text-white text-sm">
          <span>{{ getFilename(lightboxPath) }}</span>
          <span class="text-white/40">|</span>
          <button
            class="hover:text-red-400 transition-colors"
            @click="deleteImage(lightboxPath)"
          >
            删除
          </button>
          <span class="text-white/40">|</span>
          <span class="text-white/60"
            >{{ screenshots.indexOf(lightboxPath) + 1 }} / {{ screenshots.length }}</span
          >
        </div>
      </div>
    </Teleport>
  </div>
</template>
