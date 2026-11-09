<script setup lang="ts" generic="T extends string">
/**
 * 自定义下拉：菜单 Teleport 到 body，避免被浮动面板 overflow / 层叠上下文裁切
 * （设置页音质、本地排序等）。
 */
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import Icon from './Icon.vue';

const props = defineProps<{
  modelValue: T;
  options: readonly T[];
  labels: Record<string, string>;
  /** 触发器左侧图标名（Phosphor via Icon） */
  icon?: string;
}>();

const emit = defineEmits<{
  'update:modelValue': [value: T];
  change: [value: T];
}>();

const open = ref(false);
const root = ref<HTMLElement | null>(null);
const menuEl = ref<HTMLElement | null>(null);
const menuStyle = ref<Record<string, string>>({});

function placeMenu() {
  const el = root.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  const approxItem = 36;
  const pad = 8;
  const want = Math.min(280, props.options.length * approxItem + 8);
  const spaceBelow = window.innerHeight - r.bottom - pad;
  const spaceAbove = r.top - pad;
  const openUp = spaceBelow < want && spaceAbove > spaceBelow;
  const width = Math.max(r.width, 160);
  const left = Math.max(pad, Math.min(r.left, window.innerWidth - width - pad));
  if (openUp) {
    menuStyle.value = {
      left: `${left}px`,
      width: `${width}px`,
      top: 'auto',
      bottom: `${window.innerHeight - r.top + 6}px`,
      maxHeight: `${Math.min(280, Math.max(120, spaceAbove))}px`,
    };
  } else {
    menuStyle.value = {
      left: `${left}px`,
      width: `${width}px`,
      top: `${r.bottom + 6}px`,
      bottom: 'auto',
      maxHeight: `${Math.min(280, Math.max(120, spaceBelow))}px`,
    };
  }
}

async function toggle() {
  open.value = !open.value;
  if (open.value) {
    await nextTick();
    placeMenu();
  }
}

function pick(v: T) {
  if (v !== props.modelValue) {
    emit('update:modelValue', v);
    emit('change', v);
  }
  open.value = false;
}

function onDocPointer(e: PointerEvent) {
  if (!open.value) return;
  const t = e.target as Node;
  if (root.value?.contains(t) || menuEl.value?.contains(t)) return;
  open.value = false;
}

function onReposition() {
  if (open.value) placeMenu();
}

watch(open, (o) => {
  if (!o) return;
  void nextTick(placeMenu);
});

onMounted(() => {
  document.addEventListener('pointerdown', onDocPointer, true);
  window.addEventListener('resize', onReposition);
  window.addEventListener('scroll', onReposition, true);
});
onUnmounted(() => {
  document.removeEventListener('pointerdown', onDocPointer, true);
  window.removeEventListener('resize', onReposition);
  window.removeEventListener('scroll', onReposition, true);
});
</script>

<template>
  <div ref="root" class="sm-select" :class="{ open }">
    <button
      type="button"
      class="sm-select-trigger"
      :aria-expanded="open"
      aria-haspopup="listbox"
      @click="toggle"
    >
      <Icon v-if="icon" :name="icon" :size="14" />
      <span>{{ labels[modelValue] ?? modelValue }}</span>
      <Icon class="chev" name="chevron-down" :size="14" />
    </button>
    <Teleport to="body">
      <ul
        v-if="open"
        ref="menuEl"
        class="sm-select-menu portal"
        role="listbox"
        :style="menuStyle"
      >
        <li
          v-for="opt in options"
          :key="opt"
          role="option"
          :aria-selected="modelValue === opt"
          :class="{ on: modelValue === opt }"
          tabindex="0"
          @click="pick(opt)"
          @keydown.enter.prevent="pick(opt)"
        >
          {{ labels[opt] ?? opt }}
        </li>
      </ul>
    </Teleport>
  </div>
</template>
