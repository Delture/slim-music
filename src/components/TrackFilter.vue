<script setup lang="ts">
// 歌单内筛选：紧凑搜索框，非空时显示清除按钮。
import Icon from './Icon.vue';

const model = defineModel<string>({ default: '' });
withDefaults(defineProps<{ placeholder?: string }>(), {
  placeholder: '筛选当前歌单（支持拼音 / 首字母）',
});

function clear() {
  model.value = '';
}
</script>

<template>
  <div class="tf" :class="{ filled: !!model }">
    <span class="ico" aria-hidden="true"><Icon name="search" :size="14" /></span>
    <input
      v-model="model"
      type="search"
      class="inp"
      :placeholder="placeholder"
      autocomplete="off"
      spellcheck="false"
      enterkeyhint="search"
    />
    <button
      v-if="model"
      type="button"
      class="clr"
      title="清除"
      aria-label="清除筛选"
      @click="clear"
    >
      <Icon name="x" :size="12" />
    </button>
  </div>
</template>

<style scoped>
.tf {
  display: flex;
  align-items: center;
  gap: 6px;
  height: var(--btn-h);
  padding: 0 10px 0 12px;
  border-radius: var(--btn-radius);
  background: var(--fill-secondary);
  border: 0.5px solid transparent;
  box-sizing: border-box;
  transition:
    background var(--dur-fast) var(--ease-swift),
    border-color var(--dur-fast) var(--ease-swift);
  min-width: 0;
  width: 100%;
  /* 父级 .body 是纵向 flex：绝不能 flex:1，否则会纵向撑满半个面板 */
  flex: 0 0 var(--btn-h);
  align-self: stretch;
  min-height: var(--btn-h);
  max-height: var(--btn-h);
  overflow: hidden;
  margin-bottom: var(--space-2);
}
.tf:focus-within {
  background: var(--surface-3);
  border-color: var(--border);
}
.ico {
  flex-shrink: 0;
  color: var(--muted);
  display: inline-flex;
  line-height: 0;
  opacity: 0.75;
}
.inp {
  flex: 1;
  min-width: 0;
  border: 0 !important;
  background: transparent !important;
  box-shadow: none !important;
  padding: 0 !important;
  height: auto !important;
  min-height: 0 !important;
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
  outline: none;
}
.inp::placeholder {
  color: var(--muted);
  font-weight: 450;
  opacity: 0.85;
}
/* hide native search clear (we provide our own) */
.inp::-webkit-search-cancel-button,
.inp::-webkit-search-decoration {
  -webkit-appearance: none;
  appearance: none;
}
.clr {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  min-width: 22px;
  min-height: 22px;
  padding: 0;
  border: 0;
  border-radius: var(--radius);
  background: var(--hover);
  color: var(--muted);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  line-height: 0;
  transition:
    background var(--dur-fast) var(--ease-swift),
    color var(--dur-fast) var(--ease-swift);
}
.clr:hover {
  background: var(--fill-secondary);
  color: var(--text);
}
</style>
