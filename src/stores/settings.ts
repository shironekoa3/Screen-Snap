import { defineStore } from 'pinia';
import { invoke } from '@tauri-apps/api/core';

export interface Settings {
  interval_secs: number;
  format: string;
  quality: number;
  close_to_tray: boolean;
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;

export const useSettingsStore = defineStore('settings', {
  state: () => ({
    intervalSecs: 10,
    format: 'jpg',
    quality: 80,
    /** 关闭窗口时收进托盘（false = 直接退出） */
    closeToTray: true,
    /** 开机自启，以注册表为准 */
    autoStart: false,
    autoStartLoaded: false,
    loading: false,
    loaded: false,
  }),

  actions: {
    async load() {
      if (this.loaded) return;
      this.loading = true;
      try {
        const s = await invoke<Settings>('get_settings');
        this.intervalSecs = s.interval_secs;
        this.format = s.format;
        this.quality = s.quality;
        this.closeToTray = s.close_to_tray;
        this.loaded = true;
      } catch (e) {
        console.error('加载设置失败:', e);
      } finally {
        this.loading = false;
      }
    },

    async save() {
      try {
        await invoke('save_settings', {
          settings: {
            interval_secs: this.intervalSecs,
            format: this.format,
            quality: this.quality,
            close_to_tray: this.closeToTray,
          },
        });
      } catch (e) {
        console.error('保存设置失败:', e);
      }
    },

    /** 改设置就落盘（防抖 400ms），不用等点「开始截图」 */
    saveDebounced() {
      if (!this.loaded) return;
      if (saveTimer) clearTimeout(saveTimer);
      saveTimer = setTimeout(() => {
        saveTimer = null;
        this.save();
      }, 400);
    },

    /** 开机自启状态直接读注册表，避免和 settings.json 出现两份不一致的状态 */
    async loadAutoStart() {
      try {
        this.autoStart = await invoke<boolean>('get_autostart');
        this.autoStartLoaded = true;
      } catch (e) {
        console.error('读取开机自启状态失败:', e);
      }
    },

    async setAutoStart(enabled: boolean) {
      const prev = this.autoStart;
      this.autoStart = enabled; // 先乐观更新，失败再回滚
      try {
        await invoke('set_autostart_enabled', { enabled });
      } catch (e) {
        this.autoStart = prev;
        throw e;
      }
    },
  },
});
