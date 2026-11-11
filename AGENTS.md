# AGENTS.md

本文件是给在此仓库工作的 AI 编码代理（以及人类贡献者）的入门说明。

## 基础宪法

- **提交必须经过用户同意。** 完成改动后先汇报，不要自行 `git commit`；也不要自动推送或创建 PR。
- **几乎不要写注释（参考 Firefox 的工程惯例）。** 代码应当自解释：优先用清晰的
  命名、类型、模块边界和小函数表达意图。注释只用于解释“为什么”，例如非显然的
  约束、兼容性原因、安全前提或上游 API 的坑；不要写复述代码的注释、分隔线、
  注释掉的代码，也不要用注释掩盖本应重构的结构。
  例外：公开 API 的 `///` 文档、`unsafe` 的安全前提、生成代码标记和许可证头。

## 项目目标

用 Rust + [GPUI Kit](https://github.com/longbridge/gpui-kit) 重写
[`msojocs/bilibili-linux`](https://github.com/msojocs/bilibili-linux)，
做一个原生、GPU 加速的 Linux 哔哩哔哩桌面客户端。

- 目标是功能对等；具体功能以参考实现为准。
- 不照搬 Electron / 网页注入架构，用原生 Rust 重新设计实现。
- Linux 优先，UI 使用 GPUI Kit；重型能力优先使用成熟原生库，选型在实现前验证。
- 技术栈和依赖以 `Cargo.toml`、`flake.nix` 为准；目录结构以实际代码为准。

## 参考实现（只读）

`bilibili-linux/` 是参考实现的 git submodule：

- 不要修改、格式化或提交其中的文件。
- 用它对照行为、接口、文案和文档；需要更新 submodule 时单独处理。
- 参考项目是 Electron / 网页注入实现，不要照抄其架构。

## 开发环境

环境由 Nix flake 提供，`flake.nix` 是系统依赖的单一来源：

```bash
direnv allow      # 首次使用；之后进入目录自动加载
# 或者
nix develop
```

常用命令：

```bash
cargo build
cargo run                   # 运行默认成员
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

性能分析（dev shell 内，`perf` 需要以子进程方式启动应用）：

```bash
cargo build --release
perf record -e cycles:u --user-callchains -g -F 999 -o /tmp/rigili.perf -- ./target/release/rigili
perf report --stdio -i /tmp/rigili.perf -g none --no-children --sort symbol
perf script -i /tmp/rigili.perf | flamegraph.pl > /tmp/rigili.svg
heaptrack ./target/release/rigili
```

- 在 NixOS 上始终使用 dev shell 内的 Rust 和系统库，不要依赖系统 `rustc`。
- 首次构建 GPUI 较慢；运行窗口需要可用的 Wayland/X11 和 Vulkan。
- 新增系统依赖时同步更新 `flake.nix`。

## 代码约定

- 模块声明遵循 `.agents/skills/module-setup/SKILL.md`：通过项目统一的模块声明宏
  完成，禁止裸 `mod` / `pub mod` 作为模块管道。
- 提交前格式化、lint 和测试必须通过。
- 错误类型：lib crate 用 `thiserror` 定义领域错误，bin 在边界用 `anyhow` 汇总；避免
  `unwrap()` / `expect()`（启动期不变量除外）。
- 不要无理由引入新依赖；新增前确认许可证、维护状态和 Nix 构建可行性。
- 不要提交 `target/`、`.direnv/`、日志、Cookie、下载内容或缓存。
- 一个提交 / PR 只做一件事，不要顺手重构无关代码。

## UI 约定

- 开始 UI 工作前，先读内置 skills：
  `.agents/skills/gpui-kit-design-guides/references/design-guides.md`（设计规则）和
  `.agents/skills/gpui-kit/references/coding-guides.md`（编码规则）。
- UI 以 `gpui-kit` 为准，不要直接依赖其内部快照；优先复用组件和主题 token。
- 异步工作不要阻塞 UI 线程；大列表使用虚拟化组件。
- 具体 API 以当前 gpui-kit 源码和文档为准，不要凭记忆发明接口。

## API 与数据约定

- 所有 B 站请求集中在 API 层，统一处理认证、请求头、Cookie、风控、限速和错误。
- 凭据只保存在本地安全位置，日志中必须脱敏。
- 时效性数据不要长期缓存，使用前刷新。
- 网络请求设置超时、重试上限和并发上限；优先批量接口。

## 测试与验证

- 纯逻辑必须有单元测试；网络请求在测试中使用 fixture，不要访问真实接口。
- UI 交互优先使用 gpui-kit 提供的无头测试能力；需要真实窗口的功能手动验证。

## 安全与合规

- 本项目是非官方客户端；不要打包或分发 B 站专有资源，不绕过付费/会员内容。
- 漫游、空降等增强功能默认关闭或明确提示，由用户主动启用。
- 不记录、不上传用户 Cookie、观看记录或弹幕内容；联网共享功能要显式同意。
- 遵守 B 站接口频率限制；批量请求要限速并缓存。

## 代理工作流

1. 动手前先阅读相关参考实现和已有 Rust 代码，明确要复刻的行为。
2. 优先做最小可验证切片，不要一次铺开全部功能。
3. 改完运行相关检查；GUI 改动说明如何手动验证。
4. 目录、命令或依赖变化时同步更新本文件；系统依赖变化同步 `flake.nix`。
5. 不要随意升级核心 UI 依赖；需要升级时作为独立变更并说明迁移。

## 参考资料

- GPUI Kit 文档：<https://gpui-kit.com>
- GPUI Kit 仓库（含 `AGENTS.md`、skills 和示例）：<https://github.com/longbridge/gpui-kit>
- GPUI（Zed）：<https://github.com/zed-industries/zed>
- 参考实现：<https://github.com/msojocs/bilibili-linux>
- 内置 skills：`.agents/skills/`（gpui-kit skills 的来源见 `.agents/skills/README.md`）
- 模块声明规范：`.agents/skills/module-setup/SKILL.md`
- 路线图：`roadmap.md`