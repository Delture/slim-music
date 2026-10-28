<script setup lang="ts">
import type { Source } from '../lib/types';
defineProps<{ source: Source }>();
</script>

<template>
  <span class="badge" :class="source">{{ source === 'local' ? '本地' : '网易云' }}</span>
</template>

<style scoped>
.badge { font-size: 11px; padding: 1px 6px; border-radius: 4px; white-space: nowrap; font-weight: 600; }
.local { background: #e3f4e8; color: #1f7a3a; }
.netease { background: #fde4e4; color: #c62f2f; }
</style>
