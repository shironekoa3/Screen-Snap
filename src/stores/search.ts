import { defineStore } from 'pinia';
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface SearchResult {
  filepath: string;
  ocr_text: string;
  captured_at: string;
}

export const useSearchStore = defineStore('search', () => {
  const query = ref('');
  const results = ref<SearchResult[]>([]);
  const loading = ref(false);
  const searched = ref(false);

  async function search(keyword: string) {
    const trimmed = keyword.trim();
    query.value = trimmed;

    if (!trimmed) {
      results.value = [];
      searched.value = false;
      return;
    }

    loading.value = true;
    searched.value = true;
    try {
      results.value = await invoke<SearchResult[]>('search_screenshots', {
        keyword: trimmed,
      });
    } catch (e) {
      console.error('搜索失败:', e);
      results.value = [];
    } finally {
      loading.value = false;
    }
  }

  function clearSearch() {
    query.value = '';
    results.value = [];
    searched.value = false;
  }

  /** 删除截图后同步把这条结果移出列表 */
  function removeResult(filepath: string) {
    results.value = results.value.filter((r) => r.filepath !== filepath);
  }

  return { query, results, loading, searched, search, clearSearch, removeResult };
});
