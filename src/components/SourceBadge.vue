<script setup lang="ts">
import type { Source } from '../lib/types';
defineProps<{ source: Source }>();
</script>

<template>
  <span class="sm-badge badge" :class="source">{{ source === 'local' ? '本地' : '网易云' }}</span>
</template>

<style scoped>
.badge {
  height: var(--badge-h);
  padding: 0 var(--badge-px);
  border-radius: var(--badge-radius);
  font-size: var(--badge-font);
  font-weight: var(--badge-weight);
  letter-spacing: 0.01em;
  opacity: 0.92;
  justify-self: start;
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
