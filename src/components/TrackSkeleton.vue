<script setup lang="ts">
// 歌曲列表加载占位骨架：与 TrackList 行高和列宽对齐，中性灰克制呼吸动效。
withDefaults(defineProps<{ rows?: number }>(), { rows: 7 });
const titleWidths = ['46%', '34%', '54%', '40%', '50%', '36%', '44%', '38%'];
const subWidths = ['26%', '20%', '30%', '22%', '24%', '18%', '28%', '22%'];
</script>

<template>
  <div class="sk-list" aria-busy="true" aria-label="正在加载歌曲">
    <div
      v-for="i in rows"
      :key="i"
      class="sk-row"
      :style="{ '--sk-i': i - 1 }"
    >
      <span class="sk-box sk-no"></span>
      <span class="sk-box sk-cover"></span>
      <div class="sk-main">
        <span class="sk-box sk-title" :style="{ width: titleWidths[(i - 1) % titleWidths.length] }"></span>
        <span class="sk-box sk-sub" :style="{ width: subWidths[(i - 1) % subWidths.length] }"></span>
      </div>
      <span class="sk-box sk-album"></span>
      <span class="sk-box sk-badge"></span>
      <span class="sk-spacer"></span>
      <span class="sk-box sk-dur"></span>
    </div>
  </div>
</template>

<style scoped>
.sk-list {
  width: 100%;
  min-width: 0;
  pointer-events: none;
  user-select: none;
}
.sk-row {
  display: grid;
  grid-template-columns: 40px 44px minmax(0, 1fr) minmax(0, 22%) 56px 32px 52px;
  gap: var(--row-gap);
  align-items: center;
  height: 60px;
  padding: var(--space-3) var(--row-px);
  box-sizing: border-box;
  opacity: 0;
  animation: sk-enter var(--dur) var(--ease-enter) calc(var(--sk-i, 0) * 24ms) forwards;
}
.sk-box {
  display: block;
  border-radius: var(--radius-sm);
  background: var(--fill-secondary);
  animation: sk-pulse 1.5s ease-in-out calc(var(--sk-i, 0) * 80ms) infinite;
}
.sk-no {
  width: 18px;
  height: 12px;
  justify-self: center;
  opacity: 0.7;
}
.sk-cover {
  width: 40px;
  height: 40px;
  border-radius: var(--radius);
}
.sk-main {
  display: flex;
  flex-direction: column;
  gap: 7px;
  min-width: 0;
}
.sk-title {
  height: 14px;
}
.sk-sub {
  height: 11px;
  opacity: 0.72;
}
.sk-album {
  width: 68%;
  height: 12px;
  opacity: 0.75;
}
.sk-badge {
  width: 38px;
  height: 18px;
  border-radius: var(--badge-radius);
  opacity: 0.65;
}
.sk-dur {
  width: 34px;
  height: 12px;
  justify-self: end;
  opacity: 0.7;
}

@keyframes sk-enter {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}
@keyframes sk-pulse {
  0%,
  100% {
    opacity: 0.52;
  }
  50% {
    opacity: 0.92;
  }
}

@media (max-width: 700px) {
  .sk-row {
    grid-template-columns: 48px minmax(0, 1fr) 44px;
    gap: var(--space-3);
    height: 64px;
    padding: var(--space-3) var(--space-1);
  }
  .sk-no,
  .sk-album,
  .sk-badge,
  .sk-dur {
    display: none;
  }
  .sk-cover {
    width: 44px;
    height: 44px;
  }
}

@media (prefers-reduced-motion: reduce) {
  .sk-row {
    opacity: 1;
    animation: none;
  }
  .sk-box {
    animation: none;
    opacity: 0.7;
  }
}
</style>
