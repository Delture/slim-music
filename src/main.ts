import { createApp } from 'vue';
import App from './App.vue';
import './style.css';
import { applyTheme } from './lib/theme';

// 启动前先按系统偏好上色，settings 加载后再覆盖
applyTheme({ accent: 'red', themeAppearance: 'system' });

createApp(App).mount('#app');
