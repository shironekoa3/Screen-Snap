import { defineStore } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

interface CaptureResult {
  filepath: string;
  base64: string;
}

interface ScreenshotEvent {
  filepath: string;
  base64: string;
}

function getMimeType(path: string): string {
  return path.toLowerCase().endsWith('.jpg') || path.toLowerCase().endsWith('.jpeg')
    ? 'image/jpeg'
    : 'image/png';
}

export const useCaptureStore = defineStore('capture', {
  state: () => ({
    isRunning: false,
    lastScreenshot: null as string | null,
    lastScreenshotDataUrl: null as string | null,
    error: null as string | null,
    totalCount: 0,
    todayCount: 0,
    totalSizeBytes: 0,
    todaySizeBytes: 0,
    _unlisten: null as UnlistenFn | null,
  }),

  actions: {
    async start(intervalSecs: number, format: string, quality: number) {
      await invoke('start_capture', {
        intervalSecs: intervalSecs,
        format: format,
        quality: quality,
      });
      this.isRunning = true;
      this.error = null;
    },

    async stop() {
      await invoke('stop_capture');
      this.isRunning = false;
    },

    async checkStatus() {
      try {
        this.isRunning = await invoke<boolean>('get_capture_status');
      } catch (e) {
        console.error('检查状态失败:', e);
      }
    },

    async takeScreenshot(format: string, quality: number) {
      const result = await invoke<CaptureResult>('take_screenshot', {
        format,
        quality,
      });
      this.lastScreenshot = result.filepath;
      this.lastScreenshotDataUrl =
        `data:${getMimeType(result.filepath)};base64,${result.base64}`;
      this.error = null;
      await this.loadStorageInfo();
      return result.filepath;
    },

    async loadStorageInfo() {
      try {
        const info = await invoke<{
          total_size_bytes: number;
          total_count: number;
          today_size_bytes: number;
          today_count: number;
        }>('get_storage_info');
        this.totalSizeBytes = info.total_size_bytes;
        this.totalCount = info.total_count;
        this.todaySizeBytes = info.today_size_bytes;
        this.todayCount = info.today_count;
      } catch (e) {
        console.error('加载存储信息失败:', e);
      }
    },

    async setupListeners() {
      if (this._unlisten) {
        this._unlisten();
        this._unlisten = null;
      }

      const unlisten1 = await listen<ScreenshotEvent>(
        'screenshot-taken',
        (event) => {
          this.lastScreenshot = event.payload.filepath;
          this.lastScreenshotDataUrl =
            `data:${getMimeType(event.payload.filepath)};base64,${event.payload.base64}`;
          this.error = null;
          this.loadStorageInfo();
        }
      );

      const unlisten2 = await listen<string>('screenshot-error', (event) => {
        this.error = event.payload;
      });

      this._unlisten = () => {
        unlisten1();
        unlisten2();
      };
    },

    cleanup() {
      if (this._unlisten) {
        this._unlisten();
        this._unlisten = null;
      }
    },
  },
});
