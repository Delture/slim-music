<script setup lang="ts">
/**
 * App icons from Phosphor Regular (@iconify-json/ph subset, bundled).
 * Same pack + weight everywhere; play/pause use Fill for solid control buttons.
 */
import { computed } from 'vue';
import { Icon as IconifyIcon, addCollection } from '@iconify/vue';
import phSubset from '../assets/icons/ph-subset.json';

addCollection(phSubset as Parameters<typeof addCollection>[0]);

const MAP: Record<string, string> = {
  play: 'ph:play-fill',
  pause: 'ph:pause-fill',
  previous: 'ph:skip-back',
  next: 'ph:skip-forward',
  shuffle: 'ph:shuffle',
  repeat: 'ph:repeat',
  'repeat-1': 'ph:repeat-once',
  volume: 'ph:speaker-high',
  'volume-mute': 'ph:speaker-slash',
  search: 'ph:magnifying-glass',
  settings: 'ph:gear',
  list: 'ph:list-bullets',
  calendar: 'ph:calendar-blank',
  folder: 'ph:folder',
  heart: 'ph:heart',
  'heart-solid': 'ph:heart-fill',
  music: 'ph:music-notes',
  pencil: 'ph:pencil-simple',
  x: 'ph:x',
  'chevron-down': 'ph:caret-down',
  'chevron-left': 'ph:caret-left',
  plus: 'ph:plus',
  trash: 'ph:trash',
  refresh: 'ph:arrow-clockwise',
};

const props = withDefaults(
  defineProps<{ name: string; size?: number | string }>(),
  { size: 16 },
);

const icon = computed(() => MAP[props.name] ?? MAP.music);
const px = computed(() =>
  typeof props.size === 'number' ? props.size : Number.parseFloat(String(props.size)) || 16,
);
</script>

<template>
  <IconifyIcon class="icon" :icon="icon" :width="px" :height="px" aria-hidden="true" />
</template>

<style scoped>
.icon {
  display: inline-block;
  vertical-align: -0.125em;
  flex-shrink: 0;
  color: inherit;
  transition:
    transform var(--dur-fast, 120ms) cubic-bezier(0.16, 1, 0.3, 1),
    opacity var(--dur-fast, 120ms) cubic-bezier(0.16, 1, 0.3, 1);
  transform-origin: center;
}
</style>
