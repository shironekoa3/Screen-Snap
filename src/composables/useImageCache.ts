import { reactive } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export function getMimeType(path: string): string {
  const lower = path.toLowerCase();
  return lower.endsWith('.jpg') || lower.endsWith('.jpeg')
    ? 'image/jpeg'
    : 'image/png';
}

/** 缩略图缓存上限（每张几十 KB） */
const MAX_THUMBS = 500;
/** 原图缓存上限（每张约 1~2 MB，只留最近看过的几张） */
const MAX_FULLS = 6;

/**
 * 截图图片缓存。
 * - thumbs：640px 缩略图，列表/预览用（每张几十 KB）
 * - fulls：原图，只在灯箱打开时按需加载
 * 两者都做了并发去重，避免同一张图被重复 invoke。
 */
export function useImageCache() {
  const thumbs = reactive<Record<string, string>>({});
  const fulls = reactive<Record<string, string>>({});
  const pending = new Map<string, Promise<string>>();

  function loadWith(
    cache: Record<string, string>,
    cacheKey: string,
    command: string,
    filepath: string,
    mime: string,
    maxEntries: number
  ): Promise<string> {
    const hit = cache[filepath];
    if (hit) return Promise.resolve(hit);

    const inflight = pending.get(cacheKey);
    if (inflight) return inflight;

    const task = (async () => {
      try {
        const b64 = await invoke<string>(command, { filepath });
        const url = `data:${mime};base64,${b64}`;
        // 上限保护：长时间翻日期/翻灯箱时不至于把内存吃满
        if (Object.keys(cache).length >= maxEntries) {
          Object.keys(cache).forEach((k) => delete cache[k]);
        }
        cache[filepath] = url;
        return url;
      } catch (e) {
        console.error('读取图片失败:', e);
        return '';
      } finally {
        pending.delete(cacheKey);
      }
    })();

    pending.set(cacheKey, task);
    return task;
  }

  function loadThumb(filepath: string): Promise<string> {
    return loadWith(
      thumbs,
      `thumb:${filepath}`,
      'read_thumbnail_data',
      filepath,
      'image/jpeg',
      MAX_THUMBS
    );
  }

  function loadFull(filepath: string): Promise<string> {
    return loadWith(
      fulls,
      `full:${filepath}`,
      'read_screenshot_data',
      filepath,
      getMimeType(filepath),
      MAX_FULLS
    );
  }

  async function prefetchThumbs(paths: string[], concurrency = 4): Promise<void> {
    for (let i = 0; i < paths.length; i += concurrency) {
      await Promise.allSettled(paths.slice(i, i + concurrency).map(loadThumb));
    }
  }

  function reset(): void {
    Object.keys(thumbs).forEach((k) => delete thumbs[k]);
    Object.keys(fulls).forEach((k) => delete fulls[k]);
    pending.clear();
  }

  return { thumbs, fulls, loadThumb, loadFull, prefetchThumbs, reset };
}
