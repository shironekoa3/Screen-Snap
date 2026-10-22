<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
import { useSettingsStore } from '../stores/settings';

const settingsStore = useSettingsStore();
const autoStartError = ref('');

// 改完即保存（防抖），不需要额外的「保存」按钮
watch(
  () => settingsStore.closeToTray,
  () => settingsStore.saveDebounced()
);

onMounted(() => {
  if (!settingsStore.autoStartLoaded) settingsStore.loadAutoStart();
});

async function onToggleAutoStart(e: Event) {
  const enabled = (e.target as HTMLInputElement).checked;
  autoStartError.value = '';
  try {
    await settingsStore.setAutoStart(enabled);
  } catch (err: any) {
    autoStartError.value = '设置失败：' + (err?.toString() || '未知错误');
  }
}
</script>

<template>
  <div class="p-4 lg:p-8 max-w-3xl mx-auto">
    <!-- Page header -->
    <div class="mb-8">
      <h2 class="text-2xl font-bold text-slate-800">设置</h2>
      <p class="text-slate-500 text-sm mt-0.5">修改后自动保存</p>
    </div>

    <!-- 窗口行为 -->
    <section class="card p-6">
      <h3 class="text-base font-semibold text-slate-800 mb-5">窗口</h3>

      <label class="flex items-start gap-3 cursor-pointer">
        <input
          type="checkbox"
          v-model="settingsStore.closeToTray"
          class="mt-0.5 w-4 h-4 rounded accent-primary-500 cursor-pointer"
        />
        <span class="flex-1">
          <span class="block text-sm text-slate-700">关闭窗口时收进托盘</span>
          <span class="block text-xs text-slate-400 mt-1 leading-relaxed">
            {{
              settingsStore.closeToTray
                ? '关闭后程序仍在后台运行，定时截图继续；托盘图标右键可随时退出。'
                : '关闭窗口即退出程序；勾选后关闭时收进托盘，定时截图继续运行。'
            }}
          </span>
        </span>
      </label>
    </section>

    <!-- 开机自启 -->
    <section class="card p-6 mt-5">
      <h3 class="text-base font-semibold text-slate-800 mb-5">启动</h3>

      <label class="flex items-start gap-3 cursor-pointer">
        <input
          type="checkbox"
          :checked="settingsStore.autoStart"
          class="mt-0.5 w-4 h-4 rounded accent-primary-500 cursor-pointer"
          @change="onToggleAutoStart"
        />
        <span class="flex-1">
          <span class="block text-sm text-slate-700">开机自动启动</span>
          <span class="block text-xs text-slate-400 mt-1 leading-relaxed">
            开机后自动在后台运行，直接收进托盘，不弹出主窗口。
            写入的是当前用户的开机启动项，不需要管理员权限。
          </span>
        </span>
      </label>

      <p v-if="autoStartError" class="text-xs text-red-500 mt-3">
        {{ autoStartError }}
      </p>
    </section>
  </div>
</template>
