<script setup lang="ts">
/**
 * Single elegant icon set for the whole app (nav / player / settings / menus).
 * Lucide-adjacent 24×24 strokes, shared weight 1.75 / round caps — never mix emoji.
 */
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{ name: string; size?: number | string }>(),
  { size: 16 },
);

const SW = 1.75;

/** All icons on 24×24. Prefer stroke; a few solid glyphs use fill on the same grid. */
const ICONS: Record<string, { d: string | string[]; filled?: boolean }> = {
  play: {
    d: 'M8 5.75v12.5L18.25 12 8 5.75z',
    filled: true,
  },
  pause: {
    d: ['M7.25 6.5h2.5v11h-2.5z', 'M14.25 6.5h2.5v11h-2.5z'],
    filled: true,
  },
  previous: {
    d: ['M6 6.5v11', 'M18 6.5v11L9.5 12 18 6.5z'],
  },
  next: {
    d: ['M18 6.5v11', 'M6 6.5v11L14.5 12 6 6.5z'],
  },
  shuffle: {
    d: ['M16 4h4v4', 'M20 4l-6.5 6.5', 'M4 9l4 4', 'M16 20h4v-4', 'M20 20l-6.5-6.5', 'M4 15l3.5-3.5'],
  },
  repeat: {
    d: ['M17 2l3 3-3 3', 'M4 7h14a4 4 0 0 1 0 8h-2', 'M7 22l-3-3 3-3', 'M20 17H6a4 4 0 0 1 0-8h2'],
  },
  'repeat-1': {
    d: [
      'M17 2l3 3-3 3',
      'M4 7h14a4 4 0 0 1 0 8h-2',
      'M7 22l-3-3 3-3',
      'M20 17H6a4 4 0 0 1 0-8h2',
      'M12 10.5v5',
      'M12 10.5c.55-.65 1.15-.95 1.85-.95',
    ],
  },
  volume: {
    d: [
      'M4 9.5h3.25L11 6.5v11L7.25 14.5H4V9.5z',
      'M15.2 8.6a3.8 3.8 0 0 1 0 6.8',
      'M17.6 6a6.5 6.5 0 0 1 0 12',
    ],
  },
  'volume-mute': {
    d: ['M4 9.5h3.25L11 6.5v11L7.25 14.5H4V9.5z', 'M16 9.5l4 4', 'M20 9.5l-4 4'],
  },
  search: {
    d: ['M10.5 5a5.5 5.5 0 1 1 0 11 5.5 5.5 0 0 1 0-11z', 'M15.5 15.5L20 20'],
  },
  settings: {
    d: [
      'M12 9a3 3 0 1 1 0 6 3 3 0 0 1 0-6z',
      'M12 3.5v2',
      'M12 18.5v2',
      'M5.05 6.55l1.4 1.4',
      'M17.55 16.05l1.4 1.4',
      'M3.5 12h2',
      'M18.5 12h2',
      'M5.05 17.45l1.4-1.4',
      'M17.55 7.95l1.4-1.4',
    ],
  },
  list: {
    d: ['M9 7h11', 'M9 12h11', 'M9 17h11', 'M4.5 7h.01', 'M4.5 12h.01', 'M4.5 17h.01'],
  },
  calendar: {
    d: [
      'M7.5 4v2.5',
      'M16.5 4v2.5',
      'M4.5 9h15',
      'M6 6.5h12a1.5 1.5 0 0 1 1.5 1.5v10A1.5 1.5 0 0 1 18 19.5H6A1.5 1.5 0 0 1 4.5 18V8A1.5 1.5 0 0 1 6 6.5z',
    ],
  },
  folder: {
    d: 'M3.75 8A1.75 1.75 0 0 1 5.5 6.25h3.8l1.7 1.75H18.5A1.75 1.75 0 0 1 20.25 9.75v7.5A1.75 1.75 0 0 1 18.5 19H5.5A1.75 1.75 0 0 1 3.75 17.25V8z',
  },
  heart: {
    d: 'M12 20S4 14.8 4 9.6A3.9 3.9 0 0 1 12 7.2a3.9 3.9 0 0 1 8 2.4C20 14.8 12 20 12 20z',
  },
  'heart-solid': {
    d: 'M12 20S4 14.8 4 9.6A3.9 3.9 0 0 1 12 7.2a3.9 3.9 0 0 1 8 2.4C20 14.8 12 20 12 20z',
    filled: true,
  },
  music: {
    d: 'M9 18a2.5 2.5 0 1 1-1.4-2.24V7.2l9.4-2.2v8.74A2.5 2.5 0 1 1 15.6 16V8.15L9 9.65V18z',
  },
  pencil: {
    d: ['M12.5 5.5l6 6', 'M4 16.5L14.5 6l3.5 3.5L7.5 20H4v-3.5z'],
  },
  x: {
    d: ['M6.5 6.5l11 11', 'M17.5 6.5l-11 11'],
  },
  'chevron-down': {
    d: 'M6.5 9.5L12 15l5.5-5.5',
  },
  'chevron-left': {
    d: 'M14.5 6.5L9 12l5.5 5.5',
  },
};

const icon = computed(() => ICONS[props.name] ?? ICONS.music);
const px = computed(() => (typeof props.size === 'number' ? `${props.size}px` : props.size));
const paths = computed(() => {
  const d = icon.value.d;
  return Array.isArray(d) ? d : [d];
});
const filled = computed(() => !!icon.value.filled);
</script>

<template>
  <svg
    class="icon"
    viewBox="0 0 24 24"
    :width="px"
    :height="px"
    aria-hidden="true"
    focusable="false"
  >
    <path
      v-for="(p, i) in paths"
      :key="i"
      :d="p"
      :fill="filled ? 'currentColor' : 'none'"
      :stroke="filled ? 'none' : 'currentColor'"
      :stroke-width="filled ? undefined : SW"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
</template>

<style scoped>
.icon {
  display: inline-block;
  vertical-align: -0.125em;
  flex-shrink: 0;
  transition:
    transform var(--dur-fast, 120ms) cubic-bezier(0.16, 1, 0.3, 1),
    opacity var(--dur-fast, 120ms) cubic-bezier(0.16, 1, 0.3, 1);
  transform-origin: center;
}
</style>
