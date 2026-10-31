<script setup lang="ts">
import { nextTick, ref, watch, onMounted, onUnmounted } from 'vue';
import { dialogState as d, closeDialog } from '../lib/dialog';

const inputEl = ref<HTMLInputElement | null>(null);
const okEl = ref<HTMLButtonElement | null>(null);

watch(() => d.open, async (o) => {
  if (!o) return;
  await nextTick();
  if (d.kind === 'prompt') { inputEl.value?.focus(); inputEl.value?.select(); }
  else okEl.value?.focus();
});

function onKey(e: KeyboardEvent) {
  if (!d.open) return;
  if (e.key === 'Escape') { e.preventDefault(); closeDialog(false); }
  else if (e.key === 'Enter' && !e.isComposing) {
    if (d.kind === 'prompt' && !d.value.trim()) return;
    e.preventDefault(); closeDialog(true);
  }
}
onMounted(() => window.addEventListener('keydown', onKey, true));
onUnmounted(() => window.removeEventListener('keydown', onKey, true));
</script>

<template>
  <Teleport to="body">
    <Transition name="sm-dlg">
      <div v-if="d.open" class="sm-dlg-backdrop" @pointerdown.self="closeDialog(false)">
        <div class="sm-dlg" role="dialog" aria-modal="true" :aria-label="d.title">
          <div class="sm-dlg-title">{{ d.title }}</div>
          <div v-if="d.message" class="sm-dlg-msg">{{ d.message }}</div>
          <input v-if="d.kind === 'prompt'" ref="inputEl" v-model="d.value" class="sm-dlg-input"
                 type="text" spellcheck="false" autocomplete="off" />
          <div class="sm-dlg-actions">
            <button v-if="d.kind !== 'alert'" type="button" @click="closeDialog(false)">取消</button>
            <button ref="okEl" type="button" :class="d.danger ? 'btn-danger' : 'btn-primary'"
                    :disabled="d.kind === 'prompt' && !d.value.trim()" @click="closeDialog(true)">
              {{ d.confirmText }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.sm-dlg-backdrop {
  position: fixed; inset: 0; z-index: 10000;
  display: flex; align-items: center; justify-content: center;
  background: rgba(0, 0, 0, 0.28);
  padding: max(16px, env(safe-area-inset-top)) 16px max(16px, env(safe-area-inset-bottom));
}
.sm-dlg {
  width: 360px; max-width: 100%;
  box-sizing: border-box;
  background: var(--panel-bg);
  border: var(--panel-border);
  border-radius: var(--radius-xl);
  box-shadow: var(--panel-shadow);
  padding: 20px;
  display: flex; flex-direction: column; gap: 12px;
  font-family: var(--font-ui);
  color: var(--text);
}
.sm-dlg-title { font-size: 15px; font-weight: 600; font-family: var(--font-display); }
.sm-dlg-msg {
  font-size: 13px; line-height: 1.5; color: var(--text-secondary);
  white-space: pre-wrap; word-break: break-word;
  max-height: 40vh; overflow: auto;
}
.sm-dlg-input { width: 100%; box-sizing: border-box; }
.sm-dlg-actions { display: flex; justify-content: flex-end; gap: var(--btn-gap); margin-top: 4px; }
.sm-dlg-actions button { min-width: 72px; padding: 0 var(--btn-px); }

.sm-dlg-enter-active, .sm-dlg-leave-active { transition: opacity 180ms var(--ease-swift, ease); }
.sm-dlg-enter-active .sm-dlg, .sm-dlg-leave-active .sm-dlg {
  transition: transform 180ms var(--ease-swift, ease), opacity 180ms var(--ease-swift, ease);
}
.sm-dlg-enter-from, .sm-dlg-leave-to { opacity: 0; }
.sm-dlg-enter-from .sm-dlg, .sm-dlg-leave-to .sm-dlg { transform: translateY(6px) scale(0.97); opacity: 0; }

@media (max-width: 700px) {
  .sm-dlg { width: 100%; max-width: 420px; border-radius: var(--radius-xl); padding: 20px 18px 18px; }
  .sm-dlg-actions button { flex: 1; min-height: 44px; height: 44px; }
  .sm-dlg-input { min-height: 44px; font-size: 16px; }
}
</style>
