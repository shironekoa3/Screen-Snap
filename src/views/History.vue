<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useImageCache } from '../composables/useImageCache';
import { useSearchStore } from '../stores/search';

const searchStore = useSearchStore();

interface Item {
  filepath: string;
  ocrText?: string;
}

const dates = ref<string[]>([]);
const selectedDate = ref('');
const screenshots = ref<string[]>([]);
/** screenshots 对应的是哪一天，避免重复读取、也避免切回浏览时闪加载态 */
const screenshotsDate = ref('');
const loadingDates = ref(true);
const loadingImages = ref(false);
const keyword = ref('');
const lightboxImage = ref<string | null>(null);
const lightboxPath = ref<string>('');
const lightboxOcrText = ref<string>('');
const confirmTarget = ref<string | null>(null);

// 列表用缩略图，灯箱才按需读原图
const { thumbs, loadFull, prefetchThumbs } = useImageCache();

let debounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 有关键词就是搜索模式，否则是浏览模式 */
const isSearchMode = computed(() => keyword.value.trim().length > 0);

/** 从文件路径取日期：.../screenshots/2026-10-22/18-46-35-412.jpg */
function dateOf(filepath: string): string {
  const m = filepath.match(/(\d{4}-\d{2}-\d{2})[/\\][^/\\]+$/);
  return m ? m[1] : '未知日期';
}

/** 搜索结果按日期分组，最新日期在前 */
const grouped = computed(() => {
  const map = new Map<string, Item[]>();
  for (const r of searchStore.results) {
    const date = dateOf(r.filepath);
    const item: Item = { filepath: r.filepath, ocrText: r.ocr_text };
    const list = map.get(date);
    if (list) list.push(item);
    else map.set(date, [item]);
  }
  return Array.from(map.entries())
    .map(([date, items]) => ({ date, items }))
    .sort((a, b) => b.date.localeCompare(a.date));
});

/** 当前要展示的截图：搜索模式下来自结果，浏览模式下来自磁盘 */
const items = computed<Item[]>(() => {
  if (isSearchMode.value) {
    return grouped.value.find((g) => g.date === selectedDate.value)?.items ?? [];
  }
  return screenshots.value.map((fp) => ({ filepath: fp }));
});

/** 左侧日期列表；搜索模式带命中数量 */
const dateEntries = computed(() => {
  if (isSearchMode.value) {
    return grouped.value.map((g) => ({ date: g.date, count: g.items.length }));
  }
  return dates.value.map((d) => ({ date: d, count: 0 }));
});

const searchLoading = computed(
  () => isSearchMode.value && (searchStore.loading || !searchStore.searched)
);

const searchEmpty = computed(
  () =>
    isSearchMode.value &&
    searchStore.searched &&
    !searchStore.loading &&
    searchStore.results.length === 0
);

async function loadDates() {
  loadingDates.value = true;
  try {
    dates.value = await invoke<string[]>('get_dates_with_screenshots');
  } catch (e) {
    console.error('加载日期失败:', e);
  } finally {
    loadingDates.value = false;
  }
}

async function loadScreenshots(force = false) {
  const date = selectedDate.value;
  if (!date) {
    screenshots.value = [];
    screenshotsDate.value = '';
    return;
  }
  // 这一天已经读过就不重读：从搜索切回浏览时不会闪一下「加载中」
  if (!force && screenshotsDate.value === date) return;

  loadingImages.value = true;
  try {
    const files = await invoke<string[]>('get_screenshots_by_date', { date });
    // 期间用户可能又切了日期，丢弃过期结果
    if (selectedDate.value !== date) return;
    screenshots.value = files;
    screenshotsDate.value = date;
  } catch (e) {
    console.error('加载截图列表失败:', e);
    screenshots.value = [];
    screenshotsDate.value = '';
  } finally {
    loadingImages.value = false;
  }
}

function refresh() {
  loadDates();
  if (isSearchMode.value) {
    searchStore.search(keyword.value);
  } else {
    loadScreenshots(true);
  }
}

function onInput() {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    if (isSearchMode.value) searchStore.search(keyword.value);
    else searchStore.clearSearch();
  }, 300);
}

function onClearSearch() {
  keyword.value = '';
  searchStore.clearSearch();
}

async function openLightbox(filepath: string) {
  lightboxPath.value = filepath;
  // 先用缩略图占位，原图加载完再替换，避免开灯箱时白屏
  lightboxImage.value = thumbs[filepath] || null;
  lightboxOcrText.value = '';
  // Fetch image and OCR text in parallel
  const [imageUrl, ocrText] = await Promise.all([
    loadFull(filepath),
    invoke<string | null>('get_ocr_text', { filepath }).catch(() => null),
  ]);
  // 用户可能已经翻到别的图，别把旧结果写回来
  if (lightboxPath.value !== filepath) return;
  lightboxImage.value = imageUrl || thumbs[filepath] || null;
  lightboxOcrText.value = ocrText || '';
}

function closeLightbox() {
  lightboxImage.value = null;
  lightboxPath.value = '';
  lightboxOcrText.value = '';
}

/** 删除前先弹确认，避免误点 */
function requestDelete(filepath: string) {
  confirmTarget.value = filepath;
}

async function confirmDelete() {
  const filepath = confirmTarget.value;
  confirmTarget.value = null;
  if (!filepath) return;
  try {
    await invoke('delete_screenshot', { filepath });
    screenshots.value = screenshots.value.filter((f) => f !== filepath);
    // 搜索模式下同步从结果里移除，日期分组会跟着重算
    searchStore.removeResult(filepath);
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
  if (!/^\d{4}-\d{2}-\d{2}$/.test(dateStr)) return dateStr;
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
  // filename like "14-30-25-412.png"
  return filename.replace(/\.(png|jpg|jpeg)$/i, '');
}

function truncateText(text: string, maxLen: number = 80): string {
  if (!text) return '';
  return text.length > maxLen ? text.slice(0, maxLen) + '...' : text;
}

// Handle keyboard shortcuts in lightbox
function onKeydown(e: KeyboardEvent) {
  if (!lightboxPath.value) return;
  if (e.key === 'Escape') {
    closeLightbox();
  } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    const idx = items.value.findIndex((i) => i.filepath === lightboxPath.value);
    if (e.key === 'ArrowLeft' && idx > 0) {
      openLightbox(items.value[idx - 1].filepath);
    } else if (e.key === 'ArrowRight' && idx < items.value.length - 1) {
      openLightbox(items.value[idx + 1].filepath);
    }
  }
}

// 当前日期的条目变化：预取这一批缩略图
// 注意：搜索结果变化时不要清空缩略图缓存——清空会让整个列表退回「加载中」，
// 从搜索切回浏览时体验很差。缓存本身有上限（见 useImageCache）。
watch(items, (list) => {
  if (list.length > 0) prefetchThumbs(list.map((i) => i.filepath));
});

// 日期列表变化：保证 selectedDate 始终有效
watch(
  dateEntries,
  (list) => {
    if (list.length === 0) {
      selectedDate.value = '';
      return;
    }
    if (!list.some((e) => e.date === selectedDate.value)) {
      selectedDate.value = list[0].date;
    }
  },
  { immediate: true }
);

// 浏览模式下切换日期要重新读磁盘；搜索模式的结果已经在内存里
watch([selectedDate, isSearchMode], () => {
  if (!isSearchMode.value) loadScreenshots();
});

onMounted(() => {
  loadDates();
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
    <div class="flex items-center justify-between mb-6">
      <div>
        <h2 class="text-2xl font-bold text-slate-800">历史记录</h2>
        <p class="text-slate-500 text-sm mt-0.5">
          按日期浏览截图，或搜索截图中的文字
        </p>
      </div>
      <button class="btn-ghost text-xs" @click="refresh" title="刷新数据">
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
        placeholder="搜索截图中的文字，留空则按日期浏览..."
        class="w-full pl-12 pr-10 py-3 bg-white border-2 border-slate-200 focus:border-primary-400 rounded-xl text-sm text-slate-800 placeholder-slate-400 focus:outline-none focus:ring-4 focus:ring-primary-500/10 transition-all duration-200"
        @input="onInput"
      />
      <button
        v-if="keyword"
        class="absolute right-4 top-1/2 -translate-y-1/2 p-1 rounded-full text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition-colors"
        @click="onClearSearch"
        title="清空搜索"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- 搜索中 -->
    <div
      v-if="searchLoading"
      class="card flex items-center justify-center py-20"
    >
      <svg
        class="w-8 h-8 text-primary-500 animate-spin"
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

    <!-- 搜索无结果 -->
    <div
      v-else-if="searchEmpty"
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

    <!-- 浏览模式：没有任何截图 -->
    <div
      v-else-if="!isSearchMode && !loadingDates && dates.length === 0"
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

          <div v-if="loadingDates && !isSearchMode" class="space-y-2">
            <div
              v-for="i in 3"
              :key="i"
              class="h-9 bg-slate-100 rounded-lg animate-pulse"
            ></div>
          </div>

          <div v-else class="flex lg:block gap-2 lg:gap-0 lg:space-y-1 lg:max-h-[560px] overflow-x-auto lg:overflow-x-hidden overflow-y-hidden lg:overflow-y-auto pr-1">
            <button
              v-for="entry in dateEntries"
              :key="entry.date"
              @click="selectedDate = entry.date"
              class="lg:w-full text-left px-3 py-2.5 rounded-lg text-sm transition-all duration-150 flex-shrink-0"
              :class="
                selectedDate === entry.date
                  ? 'bg-primary-50 text-primary-700 font-medium'
                  : 'text-slate-600 hover:bg-slate-50'
              "
            >
              <div class="flex items-center justify-between gap-2">
                <span>{{ formatDate(entry.date) }}</span>
                <span v-if="isSearchMode" class="text-xs opacity-60">
                  {{ entry.count }}
                </span>
              </div>
              <div class="text-xs mt-0.5 opacity-60">{{ entry.date }}</div>
            </button>
          </div>
        </div>
      </div>

      <!-- Image grid -->
      <div class="flex-1 min-w-0">
        <div class="card p-5 min-h-[500px]">
          <!-- Selected date header -->
          <div class="flex items-center justify-between gap-3 mb-4">
            <h3 class="text-sm font-semibold text-slate-600">
              <template v-if="isSearchMode">
                {{ selectedDate }} 的搜索结果
              </template>
              <template v-else>
                {{ selectedDate ? `${selectedDate} 的截图` : '请选择日期' }}
              </template>
              <span
                v-if="items.length > 0"
                class="text-slate-400 font-normal ml-2"
              >
                · {{ items.length }} 张
              </span>
            </h3>
            <p v-if="isSearchMode" class="text-xs text-slate-400 flex-shrink-0">
              共 {{ searchStore.results.length }} 个结果 ·
              「{{ searchStore.query }}」
            </p>
          </div>

          <!-- Loading -->
          <div v-if="loadingImages && !isSearchMode" class="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <div
              v-for="i in 8"
              :key="i"
              class="aspect-video bg-slate-100 rounded-lg animate-pulse"
            ></div>
          </div>

          <!-- Empty for selected date -->
          <div
            v-else-if="items.length === 0"
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
              v-for="item in items"
              :key="item.filepath"
              class="group cursor-pointer"
              @click="openLightbox(item.filepath)"
            >
              <!-- Thumbnail -->
              <div
                class="relative aspect-video rounded-lg overflow-hidden bg-slate-100 border-2 border-transparent hover:border-primary-400 transition-all duration-150 shadow-sm hover:shadow-md"
                :class="isSearchMode ? 'mb-2' : ''"
              >
                <img
                  v-if="thumbs[item.filepath]"
                  :src="thumbs[item.filepath]"
                  :alt="getFilename(item.filepath)"
                  loading="lazy"
                  class="w-full h-full object-cover transition-transform duration-200 group-hover:scale-105"
                />

                <!-- Loading placeholder -->
                <div
                  v-if="!thumbs[item.filepath]"
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
                      {{ getTimeFromPath(item.filepath) }}
                    </span>
                    <button
                      class="p-1.5 rounded-md bg-white/20 hover:bg-red-500 text-white transition-colors duration-150"
                      title="删除"
                      @click.stop="requestDelete(item.filepath)"
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

              <!-- OCR snippet（仅搜索模式） -->
              <p
                v-if="isSearchMode"
                class="text-xs text-slate-500 leading-relaxed px-0.5 line-clamp-2"
              >
                {{ truncateText(item.ocrText || '') || '[未识别到文字]' }}
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Lightbox modal -->
    <Teleport to="body">
      <div
        v-if="lightboxPath"
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
              const idx = items.findIndex((i) => i.filepath === lightboxPath);
              if (idx > 0) openLightbox(items[idx - 1].filepath);
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
              const idx = items.findIndex((i) => i.filepath === lightboxPath);
              if (idx < items.length - 1) openLightbox(items[idx + 1].filepath);
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
            v-if="lightboxImage"
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
            @click="requestDelete(lightboxPath)"
          >
            删除
          </button>
          <span class="text-white/40">|</span>
          <span class="text-white/60"
            >{{ items.findIndex((i) => i.filepath === lightboxPath) + 1 }} / {{ items.length }}</span
          >
        </div>
      </div>
    </Teleport>

    <!-- 删除确认 -->
    <Teleport to="body">
      <div
        v-if="confirmTarget"
        class="fixed inset-0 z-[60] bg-black/40 flex items-center justify-center p-4"
        @click.self="confirmTarget = null"
      >
        <div class="card w-full max-w-sm p-5">
          <h3 class="text-base font-semibold text-slate-800 mb-2">
            删除这张截图？
          </h3>
          <p class="text-sm text-slate-500 mb-1 break-all">
            {{ getFilename(confirmTarget || '') }}
          </p>
          <p class="text-xs text-slate-400 mb-5">
            文件、缩略图与识别文字会一起删除，无法恢复。
          </p>
          <div class="flex justify-end gap-2">
            <button class="btn-ghost" @click="confirmTarget = null">取消</button>
            <button class="btn-danger" @click="confirmDelete">删除</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>
