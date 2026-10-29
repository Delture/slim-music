# SlimMusic

自用的精简音乐播放器：**本地音乐 + 网易云（每日推荐 / 搜索 / 我的歌单）**。
Tauri 2 + Rust + Vue 3，目标平台 macOS、Windows。没有 Node 侧车，也不打包 NeteaseCloudMusicApi，网易云接口直接在 Rust 里调用。

## 功能

| 页面 | 说明 |
|---|---|
| 每日推荐 | 需要登录 |
| 搜索 | 单曲搜索，可加载更多 |
| 我的歌单 | 左侧是歌单列表，默认打开「我喜欢的音乐」 |
| 本地音乐 | 扫描设置里的文件夹，用 lofty 读标签，可筛选 |
| 设置 | 扫码登录 / 退出、网易云音质（standard / higher / exhigh / lossless / hires）、本地文件夹 |

- **红心 / 取消喜欢**、**取消收藏歌单**（收藏的别人的歌单）
- **播放页**：点底部栏封面进入，左侧大封面 + 右侧滚动歌词
- **关窗不停播**：点红灯隐藏窗口，托盘/菜单栏继续；托盘右键：显示、播放/暂停、上下首、退出。macOS 菜单栏可显示当前歌词
- 进度条用 rAF 写 DOM，拖动时不跟 timeupdate 抢

播放规则：**双击行**、**单击序号 ▶ 或封面**、右键 **「从这首开始播放」**，都会把当前整张列表设为播放队列，从该曲开始往后播。
右键「添加到播放队列末尾」可以把本地和网易云歌曲混在一个队列里。
支持上一首 / 下一首 / 进度拖动 / 音量 / 随机 / 循环（列表、单曲）。每首歌在列表和播放条上都带「本地 / 网易云」来源标记。

## 开发与构建

### 准备环境

**macOS**
```bash
xcode-select --install
curl https://sh.rustup.rs -sSf | sh      # Rust
brew install node                        # Node 20+
```

**Windows**
- 安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，勾选「使用 C++ 的桌面开发」
- 安装 [Rust](https://rustup.rs/)（msvc 工具链）和 Node 20+
- WebView2：Windows 10/11 一般已自带

### 运行
```bash
npm install
npm run tauri dev        # 开发模式（Vite 热更新 + Rust）
npm run tauri build      # 打包：macOS 生成 .app/.dmg，Windows 生成 NSIS 安装包
```
其他命令：`npm run typecheck`（前端类型检查）；`cd src-tauri && cargo check`；`cargo test -- --ignored`（联网冒烟测试：匿名搜索 + 获取二维码 key）。

数据保存在系统应用数据目录（identifier `dev.slimmusic.app`）：
- `settings.json`：音质、文件夹、音量
- `cookie.txt`：网易云登录 cookie，明文保存，仅限自用

## 架构

```
src/                      前端 (Vue 3 + TS，无路由/状态库)
  App.vue                 侧边栏 + 页面注册表（增删页面只改这里）
  lib/types.ts            Track / Playlist / Settings 类型（与 Rust model.rs 对应）
  lib/api.ts              所有 invoke 调用
  lib/player.ts           播放器：单个 <audio> + 队列/顺序/随机/循环
  lib/session.ts          登录态、设置
  components/TrackList.vue  通用歌曲列表（播放入口、右键菜单）
  components/PlayerBar.vue  底部播放条
  components/SourceBadge.vue 来源标记
  views/*.vue             五个页面
src-tauri/src/
  lib.rs                  #[tauri::command] 注册、AppState（cookie、设置）
  ncm.rs                  网易云接口（基于 ncm-api-rs）→ 统一 Track
  local.rs                本地扫描（walkdir + lofty）
  settings.rs             设置 / cookie 的 JSON 持久化
  model.rs                Track { source: Local | Netease, ... }
```

- 本地歌曲：`convertFileSrc(path)` 走 Tauri asset 协议，交给 `<audio>` 播放。`tauri.conf.json` 里 assetProtocol 的 scope 是 `**`，自用够了，想收紧可以改成你的音乐目录。
- 网易云歌曲：播放时才调用 `song_url`，按设置里的音质取直链；取不到（无版权 / 需要 VIP）就自动跳过。
- 用到的网易云接口（ncm-api-rs 方法）：`login_qr_key`、`login_qr_check`、`user_account`、`recommend_songs`、`cloudsearch`、`user_playlist`、`playlist_track_all`、`song_url_v1`。

## TODO

- [x] 歌词播放页、红心、取消收藏歌单、托盘/关窗不停播、应用图标
- [ ] 系统媒体键：目前有 `navigator.mediaSession` + 托盘控制，原生 media key 未验证
- [ ] 本地歌曲封面 / 本地歌词
- [ ] 本地扫描结果缓存
- [ ] 保存音量、退出时的播放队列和进度
- [ ] 歌曲地址过期后的重取；列表循环空播上限
- [ ] 解灰（可参考 Yokaze unm；GPL-3.0）
- [ ] 浏览并收藏别人的歌单（目前「我的歌单」里只能取消已收藏）

## 许可

本项目代码：MIT。
依赖 `ncm-api-rs` 0.1.0（crates.io，WTFPL），作者 SPlayer-Dev，移植自 NeteaseCloudMusicApiEnhanced。
本项目没有复制 Yokaze 的代码。仅供学习自用，与网易云音乐官方无关。
