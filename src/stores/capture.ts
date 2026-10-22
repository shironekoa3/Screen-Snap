import { defineStore } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

let storageTimer: ReturnType<typeof setTimeout> | null = null;

interface CaptureResult {
  filepath: string;
  thumbnail: string;
  /** 原图还在后台编码落盘，此时文件可能尚未生成 */
  pending: boolean;
}

interface ScreenshotEvent {
  filepath: string;
  thumbnail: string;
  pending: boolean;
}

interface StorageInfo {
  total_size_bytes: number;
  total_count: number;
  today_size_bytes: number;
  today_count: number;
  data_dir: string;
}

export const useCaptureStore = defineStore('capture', {
  state: () => ({
    isRunning: false,
    lastScreenshot: null as string | null,
    /** 缩略图 data URL（不是原图） */
    lastScreenshotDataUrl: null as string | null,
    error: null as string | null,
    totalCount: 0,
    todayCount: 0,
    totalSizeBytes: 0,
    todaySizeBytes: 0,
    /** 截图实际存放目录（便携模式：程序目录） */
    dataDir: '',
    _unlisten: null as UnlistenFn | null,
  }),

  actions: {
    applyCapture(result: CaptureResult) {
      this.lastScreenshot = result.filepath;
      this.lastScreenshotDataUrl = `data:image/jpeg;base64,${result.thumbnail}`;
      this.error = null;
    },

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
      this.applyCapture(result);
      // 原图还在后台落盘，晚一点再统计，否则数字是旧的
      this.refreshStorageSoon();
      return result.filepath;
    },

    /** 启动时把磁盘上最新的一张截图读出来当预览，避免「最近截图」空白 */
    async loadLastScreenshot() {
      if (this.lastScreenshot) return;
      try {
        const last = await invoke<CaptureResult | null>('get_latest_screenshot');
        if (last) this.applyCapture(last);
      } catch (e) {
        console.error('读取最近截图失败:', e);
      }
    },

    /** 后台落盘有延迟，统计信息延后刷新，避免读到旧数字 */
    refreshStorageSoon() {
      if (storageTimer) clearTimeout(storageTimer);
      storageTimer = setTimeout(() => {
        storageTimer = null;
        this.loadStorageInfo();
      }, 1200);
    },

    async loadStorageInfo() {
      try {
        const info = await invoke<StorageInfo>('get_storage_info');
        this.totalSizeBytes = info.total_size_bytes;
        this.totalCount = info.total_count;
        this.todaySizeBytes = info.today_size_bytes;
        this.todayCount = info.today_count;
        this.dataDir = info.data_dir;
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
          this.applyCapture(event.payload);
          this.refreshStorageSoon();
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
