_default:
    @just --list

# 构建 workspace（debug）
build:
    @cargo build --workspace

# 构建应用（release）
build-release:
    @cargo build --release -p gpuili_app

# 运行应用；参数透传给 gpuili，例如 just run video.mp4
run *args:
    @cargo run -p gpuili_app -- {{args}}

# 运行 release 应用，用于性能验证
run-release *args:
    @cargo run --release -p gpuili_app -- {{args}}

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

# 播放器手动测试：只测视频
test-player video:
    GPUILI_TEST_VIDEO="{{video}}" cargo test -p gpuili_player plays_a_local_file -- --ignored --nocapture

# 播放器手动测试：视频 + 独立音频（DASH 路径）
test-player-dash video audio:
    GPUILI_TEST_VIDEO="{{video}}" GPUILI_TEST_AUDIO="{{audio}}" cargo test -p gpuili_player plays_a_local_file -- --ignored --nocapture

# 清理构建产物
clean:
    @cargo clean
