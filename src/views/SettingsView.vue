<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { refreshProfile, saveSettings, session } from '../lib/session';
import { coverSrc } from '../lib/format';
import { clearLyricMemory } from '../lib/lyrics';
import { QUALITY_LABELS, type CacheStats, type Quality } from '../lib/types';

const qrSvg = ref('');
const qrMsg = ref('');
let timer: number | undefined;

function stopPoll() {
  if (timer) clearInterval(timer);
  timer = undefined;
}

async function startLogin() {
  stopPoll();
  const { key, svg } = await api.loginQrKey();
  qrSvg.value = svg;
  qrMsg.value = '请用网易云音乐 App 扫码';
  timer = window.setInterval(async () => {
    const code = await api.loginQrCheck(key).catch(() => -1);
    if (code === 802) qrMsg.value = '已扫码，请在手机上确认';
    else if (code === 800) {
      qrMsg.value = '二维码已过期，请重新获取';
      stopPoll();
    } else if (code === 803) {
      stopPoll();
      qrSvg.value = '';
      qrMsg.value = '登录成功';
      await refreshProfile();
    }
  }, 2000);
}
onUnmounted(stopPoll);

async function logout() {
  await api.logout();
  session.profile = null;
}

async function addFolder() {
  const dir = await open({ directory: true, multiple: false });
  if (typeof dir === 'string' && !session.settings.localFolders.includes(dir)) {
    session.settings.localFolders.push(dir);
    await saveSettings();
  }
}
async function removeFolder(i: number) {
  session.settings.localFolders.splice(i, 1);
  await saveSettings();
}
const qualities = Object.keys(QUALITY_LABELS) as Quality[];
const stats = ref<CacheStats | null>(null);
const cacheBusy = ref(false);

function fmtBytes(n: number) {
  if (n < 1024) return n + ' B';
  if (n < 1024 * 1024) return (n / 1024).toFixed(1) + ' KB';
  if (n < 1024 * 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + ' MB';
  return (n / 1024 / 1024 / 1024).toFixed(2) + ' GB';
}

async function refreshStats() {
  try {
    stats.value = await api.cacheStats();
  } catch {
    stats.value = null;
  }
}

async function clearCache() {
  if (!confirm('确定清空歌单、歌词与音频缓存？本地音乐文件不受影响。')) return;
  cacheBusy.value = true;
  try {
    stats.value = await api.cacheClear();
    clearLyricMemory();
  } catch (e) {
    alert(String(e));
  } finally {
    cacheBusy.value = false;
  }
}

onMounted(refreshStats);
</script>

<template>
  <section class="settings">
    <h2>设置</h2>

    <h3>网易云账号</h3>
    <div v-if="session.profile" class="row">
      <img v-if="session.profile.avatar" :src="coverSrc(session.profile.avatar)" class="avatar" />
      <span>{{ session.profile.nickname }}</span>
      <button @click="logout">退出登录</button>
    </div>
    <div v-else>
      <button @click="startLogin">扫码登录</button>
      <!-- 正方白底 + 居中 SVG，避免 inline SVG 基线空隙造成参差白边 -->
      <div v-if="qrSvg" class="qr" v-html="qrSvg"></div>
      <p class="muted">{{ qrMsg }}</p>
    </div>

    <h3>音质（网易云）</h3>
    <select v-model="session.settings.quality" @change="saveSettings">
      <option v-for="q in qualities" :key="q" :value="q">{{ QUALITY_LABELS[q] }}</option>
    </select>
    <p class="muted">无损 / Hi-Res 需要对应会员；拿不到时网易云会返回较低音质或空地址。</p>

    <h3>本地音乐文件夹</h3>
    <div v-for="(f, i) in session.settings.localFolders" :key="f" class="row">
      <code>{{ f }}</code>
      <button @click="removeFolder(i)">移除</button>
    </div>
    <button @click="addFolder">添加文件夹</button>

    <h3>本地缓存</h3>
    <label class="row">
      <input
        type="checkbox"
        v-model="session.settings.cacheEnabled"
        @change="saveSettings"
      />
      启用歌单与音频缓存（默认开）
    </label>
    <div class="row">
      <span>音频缓存上限</span>
      <input
        type="number"
        min="100"
        step="100"
        style="width: 100px"
        v-model.number="session.settings.audioCacheMaxMb"
        @change="saveSettings"
      />
      <span>MB</span>
    </div>
    <p class="muted" v-if="stats">
      已用 {{ fmtBytes(stats.totalBytes) }}（元数据 {{ fmtBytes(stats.metaBytes) }} /
      音频 {{ fmtBytes(stats.audioBytes) }}，{{ stats.audioFiles }} 个文件）
    </p>
    <div class="row">
      <button :disabled="cacheBusy" @click="refreshStats">刷新用量</button>
      <button :disabled="cacheBusy" @click="clearCache">清空缓存</button>
    </div>
  </section>
</template>

<style scoped>
.settings h3 {
  margin-top: 24px;
}
.row {
  display: flex;
  gap: 10px;
  align-items: center;
  margin: 6px 0;
}
.avatar {
  width: 32px;
  height: 32px;
  border-radius: 50%;
}
.qr {
  width: 204px;
  height: 204px;
  margin-top: 12px;
  padding: 12px;
  box-sizing: border-box;
  background: #fff;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  line-height: 0;
}
.qr :deep(svg) {
  display: block;
  width: 100%;
  height: 100%;
  max-width: 100%;
  max-height: 100%;
}
</style>
