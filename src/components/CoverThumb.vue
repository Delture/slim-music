<script setup lang="ts">
// 列表封面：占位 → 进入视口后再请求；解码尺寸按缩略图参数缩小。
import { onMounted, onUnmounted, ref, watch } from 'vue';
import { coverSrc } from '../lib/format';
import Icon from './Icon.vue';

const props = withDefaults(
  defineProps<{
    src?: string | null;
    /** 网易云 ?param= 尺寸，默认 80y80（约 40 CSS px ×2） */
    size?: string;
    iconSize?: number;
  }>(),
  { src: null, size: '80y80', iconSize: 16 },
);

const emit = defineEmits<{ click: [e: MouseEvent] }>();

const rootEl = ref<HTMLElement | null>(null);
const active = ref(false);
const loaded = ref(false);
const failed = ref(false);
let io: IntersectionObserver | null = null;

const resolved = () => coverSrc(props.src, props.size);

function observe() {
  io?.disconnect();
  io = null;
  active.value = false;
  loaded.value = false;
  failed.value = false;
  if (!props.src) return;
  const el = rootEl.value;
  if (!el) return;
  // 无 IO 时直接加载（极老环境）
  if (typeof IntersectionObserver === 'undefined') {
    active.value = true;
    return;
  }
  io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          active.value = true;
          io?.disconnect();
          io = null;
          break;
        }
      }
    },
    { root: null, rootMargin: '120px 0px', threshold: 0.01 },
  );
  io.observe(el);
}

onMounted(() => {
  void observe();
});
onUnmounted(() => {
  io?.disconnect();
  io = null;
});
watch(
  () => props.src,
  () => observe(),
);
</script>

<template>
  <div ref="rootEl" class="cover-thumb" @click.stop="emit('click', $event)">
    <img
      v-if="src && active && !failed"
      class="img"
      :class="{ on: loaded }"
      :src="resolved()"
      alt=""
      decoding="async"
      draggable="false"
      @load="loaded = true"
      @error="failed = true"
    />
    <div v-if="!src || !active || !loaded || failed" class="ph" aria-hidden="true">
      <Icon name="music" :size="iconSize" />
    </div>
  </div>
</template>

<style scoped>
.cover-thumb {
  position: relative;
  width: 40px;
  height: 40px;
  flex-shrink: 0;
  border-radius: var(--radius);
  overflow: hidden;
  cursor: pointer;
  border: 0.5px solid rgba(0, 0, 0, 0.06);
  box-shadow: var(--shadow-sm);
  background: var(--fill-secondary);
}
.img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  opacity: 0;
  transition: opacity 0.15s ease;
}
.img.on {
  opacity: 1;
}
.ph {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  color: var(--muted);
  background: var(--fill-secondary);
}
</style>
