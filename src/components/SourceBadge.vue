<script setup lang="ts">
import type { Source } from '../lib/types';
defineProps<{ source: Source }>();
</script>

<template>
  <span class="badge" :class="source">{{ source === 'local' ? '本地' : '网易云' }}</span>
</template>

<style scoped>
.badge {
  font-size: 10px;
  padding: 2px 7px;
  border-radius: var(--radius-pill);
  white-space: nowrap;
  font-weight: 650;
  flex-shrink: 0;
  justify-self: start;
  letter-spacing: 0.01em;
  opacity: 0.92;
}
.local {
  background: color-mix(in srgb, #1f7a3a 14%, transparent);
  color: #1f7a3a;
}
.netease {
  background: var(--accent-soft);
  color: var(--accent);
}
:root[data-theme='dark'] .local,
[data-theme='dark'] .local {
  background: color-mix(in srgb, #4ade80 18%, transparent);
  color: #86efac;
}
</style>
