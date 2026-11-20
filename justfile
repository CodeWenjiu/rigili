_default:
    @just --list

# 构建 workspace（debug）
build:
    @cargo build --workspace

# 构建应用（release）
build-release:
    @cargo build --release -p rigili_app

# 运行应用；参数透传给 rigili，例如 just run video.mp4
run *args:
    @cargo run -p rigili_app -- {{args}}

# 运行 release 应用，用于性能验证
run-release *args:
    @cargo run --release -p rigili_app -- {{args}}

# 用 heaptrack 采样内存；关闭窗口后打印峰值/泄漏报告，并附 mimalloc 统计
mem-profile *args:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p rigili_app
    mkdir -p target/mem
    rm -f target/mem/heaptrack.zst
    MIMALLOC_SHOW_STATS=1 heaptrack -o target/mem/heaptrack --record-only target/release/rigili {{args}}
    heaptrack_print -f target/mem/heaptrack.zst -p 1 -l 1 -a 0 -T 0 | tee target/mem/report.txt

# 运行应用并开启视频流监视，日志异步写入 $XDG_DATA_HOME/rigili/monitor.log
# RIGILI_MPV_LOG=debug just run-monitor 可提高 mpv 日志级别
run-monitor *args:
    @cargo run -p rigili_app --features video-monitor -- {{args}}

# release 版视频流监视
run-monitor-release *args:
    @cargo run --release -p rigili_app --features video-monitor -- {{args}}

# 安装用户级桌面入口与图标；之后任务栏/启动器会按 app_id=rigili 显示图标
install-desktop:
    #!/usr/bin/env bash
    set -euo pipefail
    data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
    icon_root="$data_home/icons/hicolor"
    source_icon="packaging/linux/icons/hicolor/scalable/apps/rigili.svg"

    install -Dm644 packaging/linux/rigili.desktop "$data_home/applications/rigili.desktop"
    install -Dm644 "$source_icon" "$icon_root/scalable/apps/rigili.svg"

    if command -v rsvg-convert >/dev/null 2>&1; then
        for size in 16 24 32 48 64 128 256 512; do
            install -d "$icon_root/${size}x${size}/apps"
            rsvg-convert -w "$size" -h "$size" "$source_icon" \
                -o "$icon_root/${size}x${size}/apps/rigili.png"
        done
    fi

    if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -f "$icon_root/index.theme" ]; then
        gtk-update-icon-cache -f -t "$icon_root" >/dev/null
    fi
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$data_home/applications" >/dev/null
    fi

    echo "Installed Rigili desktop entry and icons under $data_home"

# 格式化
fmt:
    @cargo fmt --all

# 检查格式
fmt-check:
    @cargo fmt --all --check

# Clippy
lint:
    @cargo clippy --workspace --all-targets -- -D warnings

# 单元测试
test:
    @cargo test --workspace

# 提交前检查：格式 + lint + 测试
check: fmt-check lint test

# 播放器手动测试：视频，可选独立音频（DASH 路径）
test-player video audio="":
    RIGILI_TEST_VIDEO="{{video}}" RIGILI_TEST_AUDIO="{{audio}}" cargo test -p rigili_player plays_a_local_file -- --ignored --nocapture

# 清理构建产物
clean:
    @cargo clean
