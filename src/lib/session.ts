// 登录态与设置的全局响应式状态
import { reactive } from 'vue';
import { api } from './api';
import type { Profile, Settings } from './types';
import { setVolume } from './player';

export const session = reactive({
  profile: null as Profile | null,
  settings: { quality: 'exhigh', localFolders: [], volume: 0.8 } as Settings,
});

export async function initSession() {
  session.settings = await api.getSettings();
  setVolume(session.settings.volume);
  await refreshProfile();
}

export async function refreshProfile() {
  try {
    session.profile = await api.profile();
  } catch {
    session.profile = null;
  }
}

export async function saveSettings() {
  await api.saveSettings(session.settings);
}
