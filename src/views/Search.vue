<script setup lang="ts">
import { ref, reactive, watch, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useSearchStore, type SearchResult } from '../stores/search';

const searchStore = useSearchStore();

const keyword = ref('');
const lightboxImage = ref<string | null>(null);
const lightboxPath = ref<string>('');
const lightboxOcrText = ref<string>('');
const imageCache = reactive<Record<string, string>>({});
const loadingSet = new Set<string>();
let debounceTimer: ReturnType<typeof setTimeout> | null = null;

function getMimeType(path: string): string {
  return path.toLowerCase().endsWith('.jpg') || path.toLowerCase().endsWith('.jpeg')
    ? 'image/jpeg'
    : 'image/png';
}

async function getImageUrl(filepath: string): Promise<string> {
  const cached = imageCache[filepath];
  if (cached) return cached;

  if (loadingSet.has(filepath)) {
    await new Promise((r) => setTimeout(r, 100));
    if (imageCache[filepath]) return imageCache[filepath];
  }

  loadingSet.add(filepath);
  try {
    const base64 = await invoke<string>('read_screenshot_data', { filepath });
    const dataUrl = `data:${getMimeType(filepath)};base64,${base64}`;
    imageCache[filepath] = dataUrl;
    return dataUrl;
  } catch (e) {
    console.error('读取截图数据失败:', e);
    return '';
  } finally {
    loadingSet.delete(filepath);
  }
}

async function prefetchImages() {
  const concurrency = 4;
  for (let i = 0; i < searchStore.results.length; i += concurrency) {
    const batch = searchStore.results.slice(i, i + concurrency);
    await Promise.allSettled(batch.map((r) => getImageUrl(r.filepath)));
  }
}

// Watch search results and prefetch
watch(
  () => searchStore.results,
  () => {
    Object.keys(imageCache).forEach((k) => delete imageCache[k]);
    loadingSet.clear();
    if (searchStore.results.length > 0) {
      prefetchImages();
    }
  }
);

function onInput() {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    searchStore.search(keyword.value);
  }, 300);
}

function onClear() {
  keyword.value = '';
  searchStore.clearSearch();
}

async function openLightbox(item: SearchResult) {
  lightboxPath.value = item.filepath;
  lightboxOcrText.value = item.ocr_text;
  lightboxImage.value = await getImageUrl(item.filepath);
}

function closeLightbox() {
  lightboxImage.value = null;
  lightboxPath.value = '';
  lightboxOcrText.value = '';
}

function getFilename(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}

function getTimeFromPath(path: string): string {
  const filename = getFilename(path);
  return filename.replace(/\.(png|jpg|jpeg)$/i, '');
}

function truncateText(text: string, maxLen: number = 80): string {
  if (!text) return '';
  return text.length > maxLen ? text.slice(0, maxLen) + '...' : text;
}

// Keyboard shortcuts in lightbox
function onKeydown(e: KeyboardEvent) {
  if (!lightboxImage.value) return;
  if (e.key === 'Escape') {
    closeLightbox();
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    const idx = searchStore.results.findIndex(
      (r) => r.filepath === lightboxPath.value
    );
    if (e.key === 'ArrowLeft' && idx > 0) {
      openLightbox(searchStore.results[idx - 1]);
    } else if (
      e.key === 'ArrowRight' &&
      idx < searchStore.results.length - 1
    ) {
      openLightbox(searchStore.results[idx + 1]);
    }
  }
}

onMounted(() => {
  document.addEventListener('keydown', onKeydown);
});

onUnmounted(() => {
  document.removeEventListener('keydown', onKeydown);
  if (debounceTimer) clearTimeout(debounceTimer);
});
</script>

<template>
  <div class="p-4 lg:p-8 max-w-7xl mx-auto min-w-0">
    <!-- Page header -->
    <div class="mb-8">
      <h2 class="text-2xl font-bold text-slate-800">文字搜索</h2>
      <p class="text-slate-500 text-sm mt-0.5">
        搜索截图中的文字内容，快速定位目标截图
      </p>
    </div>

    <!-- Search bar -->
    <div class="relative mb-6">
      <svg
        class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-slate-400"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
        />
      </svg>
      <input
        v-model="keyword"
        type="text"
        placeholder="输入关键字搜索截图中的文字..."
        class="w-full pl-12 pr-10 py-3.5 bg-white border-2 border-slate-200 focus:border-indigo-400 rounded-xl text-sm text-slate-800 placeholder-slate-400 focus:outline-none focus:ring-4 focus:ring-indigo-500/10 transition-all duration-200"
        @input="onInput"
      />
      <button
        v-if="keyword"
        class="absolute right-4 top-1/2 -translate-y-1/2 p-1 rounded-full text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors"
        @click="onClear"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- No search yet -->
    <div
      v-if="!searchStore.searched"
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
          d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
        />
      </svg>
      <h3 class="text-lg font-semibold text-slate-600 mb-2">输入关键字开始搜索</h3>
      <p class="text-sm text-slate-400 max-w-md">
        每次截图完成后会自动 OCR 识别文字并存入数据库，您可以随时搜索。
      </p>
    </div>

    <!-- Loading -->
    <div v-else-if="searchStore.loading" class="flex items-center justify-center py-20">
      <svg
        class="w-8 h-8 text-indigo-500 animate-spin"
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

    <!-- No results -->
    <div
      v-else-if="searchStore.results.length === 0"
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
          d="M9.172 16.172a4 4 0 015.656 0M9 10h.01M15 10h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
        />
      </svg>
      <h3 class="text-lg font-semibold text-slate-600 mb-2">未找到匹配的截图</h3>
      <p class="text-sm text-slate-400 max-w-md">
        没有找到包含「<span class="text-slate-600 font-medium">{{ searchStore.query }}</span>」的截图，请尝试其他关键字。
      </p>
    </div>

    <!-- Results -->
    <div v-else>
      <!-- Results header -->
      <div class="flex items-center justify-between mb-4">
        <p class="text-sm text-slate-500">
          找到
          <span class="text-slate-700 font-semibold">{{ searchStore.results.length }}</span>
          个结果 · 关键词「<span class="text-slate-700 font-medium">{{ searchStore.query }}</span>」
        </p>
      </div>

      <!-- Results grid -->
      <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
        <div
          v-for="item in searchStore.results"
          :key="item.filepath"
          class="group cursor-pointer"
          @click="openLightbox(item)"
        >
          <!-- Thumbnail -->
          <div
            class="relative aspect-video rounded-lg overflow-hidden bg-slate-100 border-2 border-transparent hover:border-indigo-400 transition-all duration-150 shadow-sm hover:shadow-md mb-2"
          >
            <img
              v-if="imageCache[item.filepath]"
              :src="imageCache[item.filepath]"
              :alt="getFilename(item.filepath)"
              loading="lazy"
              class="w-full h-full object-cover transition-transform duration-200 group-hover:scale-105"
            />

            <!-- Loading placeholder -->
            <div
              v-if="!imageCache[item.filepath]"
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

            <!-- Hover overlay with time -->
            <div
              class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-200 flex items-end p-2"
            >
              <span class="text-white text-xs font-medium">
                {{ getTimeFromPath(item.filepath) }}
              </span>
            </div>
          </div>

          <!-- OCR text snippet -->
          <p class="text-xs text-slate-500 leading-relaxed px-0.5 line-clamp-2">
            {{ truncateText(item.ocr_text) || '[未识别到文字]' }}
          </p>
        </div>
      </div>
    </div>

    <!-- Lightbox modal -->
    <Teleport to="body">
      <div
        v-if="lightboxImage"
        class="fixed inset-0 z-50 bg-black/90 flex items-center justify-center"
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

        <!-- Nav: previous -->
        <button
          class="absolute left-5 top-1/2 -translate-y-1/2 p-2.5 rounded-lg bg-white/10 hover:bg-white/20 text-white transition-colors"
          @click.stop="
            () => {
              const idx = searchStore.results.findIndex(
                (r) => r.filepath === lightboxPath
              );
              if (idx > 0) openLightbox(searchStore.results[idx - 1]);
            }
          "
        >
          <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>

        <!-- Nav: next -->
        <button
          class="absolute right-5 top-1/2 -translate-y-1/2 p-2.5 rounded-lg bg-white/10 hover:bg-white/20 text-white transition-colors"
          @click.stop="
            () => {
              const idx = searchStore.results.findIndex(
                (r) => r.filepath === lightboxPath
              );
              if (idx < searchStore.results.length - 1)
                openLightbox(searchStore.results[idx + 1]);
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
        <div
          class="absolute bottom-5 left-1/2 -translate-x-1/2 flex items-center gap-4 px-5 py-2.5 bg-white/10 backdrop-blur-sm rounded-lg text-white text-sm"
        >
          <span>{{ getFilename(lightboxPath) }}</span>
          <span class="text-white/40">|</span>
          <span class="text-white/60"
            >{{ searchStore.results.findIndex((r) => r.filepath === lightboxPath) + 1 }} / {{ searchStore.results.length }}</span
          >
        </div>
      </div>
    </Teleport>
  </div>
</template>
