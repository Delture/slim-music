<script setup lang="ts">
import { dismissUpdate, installUpdate, updater } from '../lib/updater';
</script>

<template>
  <div v-if="updater.prompt" class="update-prompt">
    <div class="title">新版本 v{{ updater.latest }}</div>
    <template v-if="updater.downloading">
      <div class="bar"><i :style="{ width: (updater.progress < 0 ? 100 : updater.progress * 100) + '%' }" /></div>
      <div class="muted">{{ updater.progress < 0 ? '下载中…' : Math.round(updater.progress * 100) + '%' }}</div>
    </template>
    <div v-else class="actions">
      <button @click="dismissUpdate">稍后</button>
      <button class="primary" @click="installUpdate">更新</button>
    </div>
    <div v-if="updater.error" class="err">{{ updater.error }}</div>
  </div>
</template>

<style scoped>
.update-prompt {
  position: fixed;
  right: 16px;
  bottom: 96px;
  z-index: 1000;
  width: 240px;
  padding: 14px 16px;
  border-radius: var(--radius-lg);
  background: var(--surface-3);
  border: 0.5px solid var(--border);
  color: inherit;
  box-shadow: var(--shadow-lg);
  font-size: 13px;
  animation: sm-scale-in var(--dur) var(--ease-enter);
  -webkit-backdrop-filter: var(--vibrancy);
  backdrop-filter: var(--vibrancy);
}
.title {
  font-weight: 600;
  margin-bottom: 8px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.bar {
  height: 4px;
  border-radius: 2px;
  background: rgba(127, 127, 127, 0.3);
  overflow: hidden;
  margin-bottom: 6px;
}
.bar i {
  display: block;
  height: 100%;
  background: var(--accent);
}
.primary {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}
.err {
  margin-top: 6px;
  color: #e66;
  word-break: break-all;
}
</style>
