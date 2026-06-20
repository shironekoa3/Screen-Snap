import { defineStore } from 'pinia';
import { invoke } from '@tauri-apps/api/core';

export interface Settings {
  interval_secs: number;
  format: string;
  quality: number;
}

export const useSettingsStore = defineStore('settings', {
  state: () => ({
    intervalSecs: 10,
    format: 'png',
    quality: 80,
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
          },
        });
      } catch (e) {
        console.error('保存设置失败:', e);
      }
    },
  },
});
