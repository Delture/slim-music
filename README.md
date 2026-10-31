# SlimMusic

自用的精简音乐播放器：**本地音乐 + 网易云（每日推荐 / 搜索 / 我的歌单）**。
Tauri 2 + Rust + Vue 3，支持 **macOS、Windows、Android** 三端（均已实测可用）。没有 Node 侧车，也不打包 NeteaseCloudMusicApi，网易云接口直接在 Rust 里调用。

> 作者：[Yuhua Zhang](https://github.com/Delture)（Delture）。本项目在 **Grok Bot** 辅助下开发完成。

## 功能

| 页面 | 说明 |
|---|---|
| 每日推荐 | 需要登录 |
| 搜索 | 单曲搜索，可加载更多 |
| 我的歌单 | 中间歌单列 + 右侧曲目两块浮动面板；右键歌单可播放全部 / 取消收藏 / 删除 |
| 本地音乐 | 扫描设置里的文件夹（mp3/flac/m4a/aac/wav/ogg/opus/aiff/ape/wma/wv），lofty 读标签，同名 .lrc 作歌词，可筛选 |
| 播放列表 | 同四面板布局（侧栏 / 列表 / 曲目 / 底栏）；右键列表项：播放全部 / 重命名 / 删除；JSON 持久化 |
| 正在播放 | 当前播放队列 |
| 设置 | 扫码登录 / 退出、主题（浅色/深色/跟随系统）、强调色（网易云红 / 陶土橙）、网易云音质、本地文件夹、歌单/音频缓存 |

- **红心 / 取消喜欢**、**取消收藏歌单**、**删除自建歌单**
- **播放页**：点底部栏封面进入，左侧大封面 + 右侧滚动歌词（含翻译，歌词有磁盘缓存）
- **记忆**：音量、随机、循环模式持久化；启动时恢复上次队列和进度（保持暂停）
- **缓存**：歌单元数据 + 音频磁盘缓存（LRU，校验 Content-Length），设置里可调；直链卡顿会自动重试/切缓存
- **关窗不停播**：点红灯隐藏窗口，托盘/菜单栏继续；托盘右键：显示、播放/暂停、上下首、退出。macOS 菜单栏可显示当前歌词
- **Android**：后台播放、通知栏/锁屏媒体控制、耳机按键、读取系统媒体库中的本地音乐、手机专用布局、登录二维码可保存到相册或直接在网易云音乐中打开
- 进度条用 rAF 写 DOM，拖动时不跟 timeupdate 抢

播放规则：**双击行**、**单击序号 ▶ 或封面**、右键 **「从这首开始播放」**，都会把当前整张列表设为播放队列，从该曲开始往后播。
右键「添加到播放队列末尾」可以把本地和网易云歌曲混在一个队列里。
支持上一首 / 下一首 / 进度拖动 / 音量 / 随机 / 循环（列表、单曲）。每首歌在列表和播放条上都带「本地 / 网易云」来源标记。

## 下载 / 安装

到 [GitHub Releases](https://github.com/Delture/slim-music/releases) 下载对应安装包：

- **macOS**（Apple Silicon）：`.dmg`。应用未签名，首次打开请在「应用程序」里右键 →「打开」，或执行 `xattr -cr /Applications/SlimMusic.app`
- **Windows**（x64）：`-setup.exe`。若出现 SmartScreen 提示，点「更多信息」→「仍要运行」
- **Android**（arm64）：`.apk`。安装时需允许「安装未知来源应用」

### 应用内更新

0.2.0 起支持应用内更新：启动时自动检查一次，也可在「设置 → 关于」手动检查。桌面端下载后自动安装并重启；Android 下载 APK 后调起系统安装器。v0.1.0 不含更新功能，需手动安装一次 0.2.0。

## 字体许可

本软件界面使用 [OPPO Sans 4.0](https://open.oppomobile.com/new/developmentDoc/info?id=13223) 字体。版权归 OPPO 广东移动通信有限公司所有，按官方许可免费嵌入使用（含商用）；未对字体文件做改编。许可全文见 [`src/assets/fonts/OPPOSans-LICENSE.txt`](src/assets/fonts/OPPOSans-LICENSE.txt)。

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
Android 打包（需 Android SDK/NDK，并先 `npx tauri android init`）：`npx tauri android build --apk`

其他命令：`npm run typecheck`（前端类型检查）；`cd src-tauri && cargo check`；`cargo test -- --ignored`（联网冒烟测试：匿名搜索 + 获取二维码 key）。

数据保存在系统应用数据目录（identifier `dev.slimmusic.app`）：
- `settings.json`：音质、文件夹、音量、缓存、主题、强调色
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

已完成：扫码登录、每日推荐、搜索、我的歌单（取消收藏 / 删除自建）、红心、本地扫描（mp3/flac/m4a/aac/wav/ogg/opus/aiff/aif/ape/wma/wv）+ 同名 .lrc、本机播放列表 +「正在播放」、从某首开始播放、播放页歌词 + 翻译 + 歌词缓存、托盘与右键菜单、关窗不停播、歌单/音频磁盘缓存（可在设置中调整）、音量/随机/循环持久化、恢复上次队列和进度（恢复后保持暂停）、封面/歌曲地址 https 修正、自绘进度条/音量条、应用图标、Windows 实测、Android 版（后台播放 / 通知栏与锁屏控制 / 耳机按键）、三端应用内更新、CI 构建 Android APK。

- [ ] wma 等格式可扫描，但 WebView 未必能解码播放
- [ ] 本地歌曲内嵌封面
- [x] 歌曲地址过期/中途 stalled 后重取或切本地缓存（保持进度）
- [ ] 列表循环空播上限
- [ ] 解灰（可参考 Yokaze unm；GPL-3.0）
- [ ] 浏览并收藏别人的歌单

## 许可

本项目代码：MIT。
依赖 `ncm-api-rs` 0.1.0（crates.io，WTFPL），作者 SPlayer-Dev，移植自 NeteaseCloudMusicApiEnhanced。
本项目没有复制 Yokaze 的代码。仅供学习自用，与网易云音乐官方无关。
