<script setup lang="ts">
import { dialog } from '../lib/dialog';
import { isAndroid, openExternal, saveImage } from '../lib/native';
import { onMounted, onUnmounted, ref } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { refreshProfile, saveSettings, session, setAppearance } from '../lib/session';
import { coverSrc } from '../lib/format';
import { clearLyricMemory } from '../lib/lyrics';
import { clearPlaylistTrackMemory } from '../lib/playlistCache';
import { clearPlayLogAll, copyPlayLog } from '../lib/playLog';
import { checkUpdate, updater } from '../lib/updater';
import { getVersion } from '@tauri-apps/api/app';
import { QUALITY_LABELS, type AccentId, type CacheStats, type Quality, type ThemeAppearance } from '../lib/types';
import { ACCENT_COLORS, ACCENT_LABELS, THEME_LABELS } from '../lib/theme';
import Icon from '../components/Icon.vue';
import SmSelect from '../components/SmSelect.vue';

const qrSvg = ref('');
const qrUrl = ref('');
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
  qrSaved.value = false;
  qrUrl.value = `https://music.163.com/login?codekey=${key}`;
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

function onQualityChange() {
  void saveSettings();
}

function onCacheMaxChange() {
  const n = Number(session.settings.audioCacheMaxMb);
  const clamped = !Number.isFinite(n) || n <= 0 ? 2048 : Math.min(50 * 1024, Math.max(100, Math.round(n)));
  session.settings.audioCacheMaxMb = clamped;
  void saveSettings().then(() => refreshStats());
}
const accents = Object.keys(ACCENT_LABELS) as AccentId[];
const themes = Object.keys(THEME_LABELS) as ThemeAppearance[];
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
  if (!(await dialog.confirm('清空缓存', '歌单、歌词与音频缓存', { danger: true, confirmText: '清空' }))) return;
  cacheBusy.value = true;
  try {
    stats.value = await api.cacheClear();
    clearLyricMemory();
    clearPlaylistTrackMemory();
  } catch (e) {
    void dialog.alert(String(e));
  } finally {
    cacheBusy.value = false;
  }
}

onMounted(refreshStats);

const logBusy = ref(false);
async function onCopyPlayLog() {
  logBusy.value = true;
  try {
    const ok = await copyPlayLog();
    void dialog.alert(ok ? '播放诊断日志已复制到剪贴板' : '暂无日志可复制');
  } catch (e) {
    void dialog.alert(String(e));
  } finally {
    logBusy.value = false;
  }
}
async function onClearPlayLog() {
  if (!(await dialog.confirm('清空播放日志', '内存与本地 play-log.txt', { confirmText: '清空' }))) return;
  logBusy.value = true;
  try {
    await clearPlayLogAll();
    void dialog.alert('已清空');
  } catch (e) {
    void dialog.alert(String(e));
  } finally {
    logBusy.value = false;
  }
}

onMounted(async () => {
  updater.current ||= await getVersion().catch(() => '');
});
const checked = ref(false);
async function manualCheck() {
  await checkUpdate();
  checked.value = true;
}

const qrSaved = ref(false);

async function openInNcm() {
  if (!qrUrl.value) return;
  const deep = `orpheus://open?url=${encodeURIComponent(qrUrl.value)}`;
  if (!(await openExternal(deep).catch(() => false))) await openExternal(qrUrl.value).catch(() => false);
}

async function saveQr() {
  const svg = qrSvg.value;
  if (!svg) return;
  const img = new Image();
  img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  await img.decode();
  const size = 720;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext('2d')!;
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, size, size);
  ctx.drawImage(img, 0, 0, size, size);
  await saveImage(canvas.toDataURL('image/png'), `slimmusic-login-${Date.now()}.png`);
  qrSaved.value = true;
}
</script>

<template>
  <section class="settings">
    <header class="head"><h2 class="head-title">设置</h2></header>

    <div class="settings-card">
      <h3>外观</h3>
      <div class="row wrap">
        <span class="label">主题</span>
        <div class="seg" role="group" aria-label="主题">
          <button
            v-for="t in themes"
            :key="t"
            type="button"
            :class="{ on: session.settings.themeAppearance === t }"
            @click="setAppearance({ themeAppearance: t })"
          >
            {{ THEME_LABELS[t] }}
          </button>
        </div>
      </div>
      <div class="row wrap">
        <span class="label">强调色</span>
        <div class="seg" role="group" aria-label="强调色">
          <button
            v-for="a in accents"
            :key="a"
            type="button"
            :class="{ on: session.settings.accent === a }"
            @click="setAppearance({ accent: a })"
          >
            <span class="swatch" :style="{ background: ACCENT_COLORS[a] }" />
            {{ ACCENT_LABELS[a] }}
          </button>
        </div>
      </div>
      <p class="muted tip">浅色为浮动面板 + 系统灰；深色为 ChatGPT 式炭灰分层（非纯黑）。界面字体为 OPPO Sans 4.0。强调色可切换。设置写入 settings.json。</p>
    </div>

    <div class="settings-card">
      <h3>网易云账号</h3>
      <div v-if="session.profile" class="row">
        <img v-if="session.profile.avatar" :src="coverSrc(session.profile.avatar)" class="avatar" />
        <span>{{ session.profile.nickname }}</span>
        <button @click="logout">退出登录</button>
      </div>
      <div v-else>
        <button class="primary" @click="startLogin">扫码登录</button>
        <div v-if="qrSvg" class="qr" v-html="qrSvg"></div>
        <div v-if="qrSvg && isAndroid" class="row qr-actions">
          <button @click="openInNcm">在网易云音乐中打开</button>
          <button @click="saveQr">{{ qrSaved ? '已保存到相册' : '保存二维码' }}</button>
        </div>
        <p class="muted">{{ qrMsg }}</p>
      </div>
    </div>

    <div class="settings-card">
      <h3>音质（网易云）</h3>
      <SmSelect
        v-model="session.settings.quality"
        :options="qualities"
        :labels="QUALITY_LABELS"
        @change="onQualityChange"
      />
      <p class="muted tip">无损 / Hi-Res 需要对应会员；拿不到时网易云会返回较低音质或空地址。</p>
    </div>

    <div v-if="!isAndroid" class="settings-card">
      <h3>本地音乐文件夹</h3>
      <div v-for="(f, i) in session.settings.localFolders" :key="f" class="row">
        <code class="path">{{ f }}</code>
        <button @click="removeFolder(i)">移除</button>
      </div>
      <button @click="addFolder">添加文件夹</button>
    </div>

    <div class="settings-card">
      <h3>本地缓存</h3>
      <label class="row">
        <input type="checkbox" v-model="session.settings.cacheEnabled" @change="saveSettings" />
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
          @change="onCacheMaxChange"
        />
        <span>MB</span>
      </div>
      <p class="muted tip">
        听过的网易云歌曲会按当前音质把音频存到本机，再次播放直接走本地文件，不依赖会过期的在线地址。调高音质会重新缓存；已有更高或同等音质时可直接复用。默认上限 2048 MB，超出后淘汰最久未听的。清空会同时删除歌单、歌词和音频缓存，不影响本机音乐库。
      </p>
      <p class="muted tip" v-if="stats">
        已用 {{ fmtBytes(stats.totalBytes) }}（元数据 {{ fmtBytes(stats.metaBytes) }} /
        音频 {{ fmtBytes(stats.audioBytes) }}，{{ stats.audioFiles }} 个文件）
      </p>
      <div class="row">
        <button :disabled="cacheBusy" @click="refreshStats">刷新用量</button>
        <button :disabled="cacheBusy" @click="clearCache">清空缓存</button>
      </div>
    </div>

    <div class="settings-card">
      <h3>播放诊断</h3>
      <p class="muted tip">播放栏出现红字时会自动记录上下文（曲目、音源、loadedId 等），不上传。可复制后发给开发者排查。</p>
      <div class="row">
        <button :disabled="logBusy" @click="onCopyPlayLog">复制播放日志</button>
        <button :disabled="logBusy" @click="onClearPlayLog">清空日志</button>
      </div>
    </div>

    <div class="settings-card">
      <h3>关于</h3>
      <div class="row wrap">
        <span>SlimMusic v{{ updater.current }}</span>
        <button :disabled="updater.checking || updater.downloading" @click="manualCheck">
          {{ updater.checking ? '检查中…' : '检查更新' }}
        </button>
        <span v-if="checked && !updater.checking && !updater.error && !updater.available" class="muted">已是最新</span>
        <span v-if="updater.error && !updater.prompt" class="muted">{{ updater.error }}</span>
      </div>
      <p class="muted tip">
        本软件使用 OPPO Sans 4.0 字体（© OPPO，免费可商用嵌入）。图标来自 Phosphor。
      </p>
    </div>
  </section>
</template>

<style scoped>
.row {
  display: flex;
  gap: var(--space-2);
  align-items: center;
  margin: var(--space-2) 0;
}
.row.wrap { flex-wrap: wrap; }
.label {
  min-width: 56px;
  color: var(--muted);
  font-size: 13px;
}
.tip { margin: var(--space-2) 0 0; font-size: 12px; }
.avatar {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  object-fit: cover;
  box-shadow: var(--shadow-sm);
}
.path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  padding: var(--space-1) var(--space-2);
  background: var(--hover);
  border-radius: var(--radius-sm);
}
.qr {
  width: 204px;
  height: 204px;
  margin-top: 12px;
  padding: 12px;
  box-sizing: border-box;
  background: #fff;
  border-radius: var(--radius-lg);
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  line-height: 0;
  box-shadow: var(--shadow-sm);
  animation: sm-scale-in var(--dur) var(--ease-enter);
}
.qr :deep(svg) {
  display: block;
  width: 100%;
  height: 100%;
  max-width: 100%;
  max-height: 100%;
}
.seg button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  overflow: hidden;
  line-height: 1;
}
.seg button .swatch {
  width: 12px;
  height: 12px;
  flex-shrink: 0;
  /* no outer selection ring — button.on is the selected chrome */
  box-shadow: inset 0 0 0 1px var(--border-strong);
}
</style>
