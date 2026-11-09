# GPUILI Roadmap

> 用 Rust + GPUI Kit 重写 `bilibili-linux`，做一个原生、GPU 加速的 Linux 哔哩哔哩桌面客户端。
>
> 本文记录阶段性目标和已确认的架构决策；具体实现以代码、`Cargo.toml` 和 `flake.nix` 为准。
> 最后更新：2026-11-09

## 当前状态

- Rust workspace 已建立：`crates/macro`、`crates/app`、`crates/bilibili`、`crates/player`
- M0 已完成：`gpuili_player` 通过 raw libmpv FFI 做 software render，`gpuili_app` 能把帧显示为 `RenderImage` 并提供基础播放控件
- 开发环境由 `flake.nix` / `.envrc` 提供
- 已有 `AGENTS.md`、内置 GPUI Kit skills 和 `module-setup` 规范
- 尚未实现：B 站 API 客户端、真实浏览/登录流程、弹幕

## 目标与非目标

### 目标

- 以 `bilibili-linux` 为行为参考，实现核心功能对等
- 原生 Rust 实现，不复刻 Electron / 网页注入架构
- Linux 优先，UI 使用 GPUI Kit，视频使用系统原生媒体能力
- 默认安全、合规：不打包 B 站专有资源，不绕过付费/会员内容

### 非目标

- 不做 Electron 套壳或网页客户端
- 不绕过 DRM、付费内容或会员限制
- 不在日志、仓库或网络中泄露用户 Cookie/观看数据
- 不在 v1 追求所有增强功能；按里程碑逐步覆盖

## 参考项目

| 项目 | 许可 | 用途 |
| --- | --- | --- |
| [`msojocs/bilibili-linux`](https://github.com/msojocs/bilibili-linux) | MIT | 桌面端行为、增强功能、设置/托盘/更新等参考；当前为只读 submodule |
| [`xfangfang/wiliwili`](https://github.com/xfangfang/wiliwili) | GPL-3.0 | 原生 API 端点、WBI、扫码登录、mpv/弹幕/直播的数据与行为参考 |
| [`longbridge/gpui-kit`](https://github.com/longbridge/gpui-kit) | Apache-2.0 | UI 框架、组件、主题、测试能力；项目内已内置 skills |
| [`jlvihv/biliguga`](https://github.com/jlvihv/biliguga) | MIT | GPUI + raw libmpv 的 B 站客户端；`sw` render → `RenderImage` 播放器参考 |
| [`iamd3vil/jellyui`](https://github.com/iamd3vil/jellyui) | AGPL-3.0 | gpui-kit + `libmpv2`；`sw` render → `RenderImage` 集成参考 |
| [`giantpand2000/GPP`](https://github.com/giantpand2000/GPP) | MIT | GPUI + GStreamer（playbin/appsink，macOS 为主）的备选播放路线 |
| [`singh-ps/rs-wgpu-video-player`](https://github.com/singh-ps/rs-wgpu-video-player) | MIT | FFmpeg + GPUI 的备选播放路线 |
| [Zed GPUI](https://github.com/zed-industries/zed) | Apache-2.0 | `RenderImage`、窗口、元素与测试 API |

> 参考 GPL/AGPL 项目时只参考协议、接口和行为；除非本项目也采用兼容许可证，不要直接复制代码。

## 架构决策

| 决策 | 状态 | 说明 |
| --- | --- | --- |
| UI 框架 | 已定 | Rust edition 2024 + `gpui-kit` 0.9.x + `gpui-fast` |
| 模块声明 | 已定 | 通过项目统一宏声明，禁止裸 `mod` / `pub mod`；见 `module-setup` skill |
| 视频后端 | 已验证（M0） | libmpv + software render（`sw`）→ GPUI `RenderImage` |
| 视频帧合成 | 已验证（M0） | CPU BGRA 帧 → `img(frame)`；控件和弹幕作为 GPUI 元素叠加 |
| API 层 | 已定 | 所有 B 站请求集中在 `gpuili_bilibili`，统一认证、请求头、WBI、限速和错误 |
| 登录 | 待验证 | 二维码登录 + 本地 Cookie/会话持久化；具体走 Web QR 或 TV QR 在 M1 决定 |
| 许可证 | 待定 | 影响 libmpv、wiliwili 代码参考和发行方式；见“开放问题” |

### 视频播放

选择 libmpv 的原因：

- B 站 DASH 通常音视频分离，libmpv 可以用 `audio-add` 接独立音频
- 硬解、字幕/ASS、多音轨、seek、倍速、音量、网络缓存都是现成的
- 已有 GPUI + libmpv 的可行实现（`biliguga`、`jellyui`）
- 弹幕和控件可以直接叠在 GPUI 的 `img` 上，不需要原生子窗口的 z-order 处理

首版渲染路径：

```text
libmpv worker thread
  → MPV_RENDER_PARAM_SW_* / bgr0
  → 单槽帧通道（UI 落后时丢弃旧帧）
  → RenderImage
  → img(frame).object_fit(ObjectFit::Contain)
  → GPUI 控件 / 弹幕叠加
```

关键参数和行为：

- `vo=libmpv`
- `hwdec=auto-safe`，验证 `hwdec-current` 实际启用了 VA-API/NVDEC
- 渲染尺寸取 `min(源尺寸, 视口尺寸)`，避免无意义放大
- 旧帧通过 `window.drop_image(...)` 释放，避免纹理泄漏
- 后台线程运行 mpv，UI 线程只取帧、更新状态和 `cx.notify()`
- 目标：1080p60 流畅；如果软件帧路线不够，再评估备选方案

M0 实测（2026-11-09，开发机）：

- 1280×720 与 1920×1080 本地视频均可播放，`hwdec-current` 为 `vaapi-copy`
- 1080p60 release 构建约 1 核 CPU（15s 播放 user 13.9s + sys 0.7s）
- 播放/暂停、seek、音量、倍速、窗口内全屏可用；resize 后按新视口重渲染
- 纯视频 + 独立音频文件通过 `audio-add` 接入，`audio-codec-name` 为 `aac`，命令与视频同步生效
- 尚未用真实 B 站 DASH URL 验证；需要 M1/M2 获取 playurl 后补测

备选方案（按优先级）：

1. 降低渲染分辨率或帧率
2. 独立 mpv 窗口/子窗口（零拷贝，但弹幕/控件叠加更难）
3. GStreamer（LGPL，`gpui-video-player` 可参考）
4. 扩展 GPUI 的外部纹理入口（成本高，暂不计划）

### API 层

`gpuili_bilibili` 负责：

- 统一 `User-Agent`、`Referer`、Cookie、超时、重试和并发限制
- WBI 签名
- 二维码登录和会话持久化
- 风控错误（如 `-352`、`-412`）的可恢复提示
- 带类型的响应模型和 fixture 测试
- 不在单元测试中访问真实 B 站接口

参考入口：`wiliwili/include/api/bilibili/api.h`、`wiliwili/include/api/bilibili/util/wbi.hpp`、
`wiliwili/include/api/bilibili/result/*.h`。

### 应用结构

- `gpuili_app`：GPUI 入口、导航、主题、i18n、设置
- `gpuili_bilibili`：B 站 API 和认证，不依赖 UI
- `gpuili_player`：播放器抽象和 libmpv 封装，不直接发 HTTP
- `gpuili_macro`：模块声明宏

新增 crate 时保持边界清晰，不要把所有功能堆进 `gpuili_app`。

## 里程碑

M0 和 M1 可以并行；M2 依赖两者完成。

### M0：播放器 spike

状态：已完成（2026-11-09）。

目标：验证 libmpv + GPUI 是否满足基本播放需求。

- 接入 libmpv（`libmpv2` / `libmpv2-sys` 或 raw FFI）
- 实现 `MpvPlayer` worker：初始化、software render、帧通道、状态
- 播放本地文件
- 用独立视频/音频文件验证 DASH 式加载（`loadfile` + `audio-add`）
- 实现播放/暂停、seek、音量、倍速、窗口内全屏
- 验证硬解、resize、帧释放
- 记录 1080p60 下的 CPU 占用

验收结果：

- 本地文件与独立音频文件可播放；B 站 DASH 待 M1/M2 验证
- `hwdec-current` 显示 `vaapi-copy`
- 1080p60 release 构建约 1 核 CPU，可连续播放
- seek/暂停/倍速/全屏可用
- 旧帧通过 `window.drop_image` 释放，未观察到泄漏
- 结论：软件帧路线对单个 1080p60 足够；4K/高帧率尚未验证

### M1：API 核心

目标：建立可测试、可复用的 B 站 API 基础。

- HTTP client、Cookie、UA/Referer、超时/重试
- WBI 签名
- 先接一个公开接口（热门/推荐）
- 带类型的响应模型和 fixture 测试
- 二维码登录 + 会话持久化
- 风控/未登录/网络失败路径
- 限速和并发上限

验收标准：

- `cargo test` 不访问真实接口
- 手动测试可以获取真实推荐流
- 登录后重启应用能恢复会话
- 风控错误有明确提示，不静默高频重试

### M2：最小闭环

目标：完成“浏览 → 详情 → 播放”的主路径。

- App shell：侧边栏、标题栏、路由
- 首页视频卡片网格（封面、标题、UP 主、播放量）
- 加载/空/错误状态
- 视频详情页
- 获取 playurl 并交给 M0 播放器
- 基本键盘/鼠标操作

验收标准：

- 首页加载真实数据
- 点击卡片可以进入详情并播放
- 切换页面不影响播放器状态（或明确停止）
- UI 不阻塞，列表使用虚拟化

### M3：浏览与社交功能

- 搜索（关键词、类型筛选）
- 历史记录、收藏夹、稍后再看
- 动态
- 评论
- 点赞、投币、收藏
- 直播列表与直播播放
- PGC/番剧/影视基础浏览

### M4：播放增强

- 弹幕拉取、显示、发送
- 弹幕设置（速度、透明度、屏蔽词、层级）
- 字幕和多音轨
- 清晰度切换
- 直播弹幕 WebSocket
- 播放进度上报和续播

### M5：bilibili-linux 特色功能

- 漫游 / 区域限制
- 外区搜索
- 弹幕共享
- 空降助手 / SponsorBlock / AI 转写
- 托盘菜单
- 检查更新
- 无边框窗口和小分辨率全屏
- 多语言切换

增强功能默认关闭或明确提示，由用户主动启用。

### M6：打包与发布

- Nix package / AppImage / deb/rpm（按需要）
- 桌面文件、图标、MIME
- CI 构建和测试
- 发布流程与更新检查
- 第三方许可证清单

## 横切关注点

- 测试：纯逻辑单测；UI 使用 gpui-kit 无头测试；真实窗口手动验证
- 性能：大列表虚拟化；视频帧不阻塞 UI；避免每帧重建整棵元素树
- 安全：Cookie 只存本地安全位置；日志脱敏；不上传用户数据
- 合规：遵守接口频率限制；不绕过付费/会员内容
- 文档：命令、目录、依赖变化同步更新 `AGENTS.md` 和本文件
- 许可证：新增依赖前确认许可证和 Nix 构建可行性

## 开放问题

1. **项目许可证**：保持 MIT/Apache，还是采用 GPL-3.0？
   - 影响是否能直接参考/移植 wiliwili（GPL-3.0）
   - `libmpv2` / `libmpv2-sys` 是 LGPL-2.1；mpv 本体默认 GPLv2+，LGPL 模式需确认构建选项
2. **wiliwili 是否作为只读 submodule** 加入仓库，方便 agent 查 API/播放器实现？
3. **播放器性能兜底**：如果软件帧路线在 4K/60 不够，优先 GStreamer 还是独立 mpv 窗口？
4. **登录流程**：Web QR 还是 TV QR？需要在 M1 用真实接口验证。
5. **v1 功能边界**：哪些 bilibili-linux 特色功能必须首版覆盖？

## 下一步

M0 已结束，下一步进入 M1：搭建可测试的 B 站 API 核心，优先完成 HTTP 客户端、WBI 签名、一个公开接口和二维码登录。拿到 playurl 后补测真实 DASH 播放。