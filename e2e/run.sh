#!/usr/bin/env bash
# Level 4 插件 API 的端到端检查：用 wit-bindgen 为真实的 WIT 世界编出两个真实的插件组件，
# 再在 wasmtime 48 里加载，经 `mass` / `queue` / `resource-install` 来回调用一遍。
#
#   guest-ri    实现了 `resource-install` 的插件（world psys-world-v4-resource-install）
#   guest-base  没实现它的插件（world psys-world-v4）
#   host        宿主侧：按 pluginsystem 里同样的形状实现 mass / queue 两个接口，驱动上面两个插件
#
# 它不依赖 Tauri，也不碰 AppHandle，所以覆盖的是「WIT/ABI + 异步调用 + 可失败的导出查找」，
# 不覆盖 PluginCtxV4、权限弹窗、与前端的往返——那部分靠 pluginsystem 里的单元测试和真机。
#
# 环境变量：E2E_GUEST_TARGET / E2E_HOST_TARGET 指定两个 cargo target 目录（默认在 e2e/target 下）。
# 首次要编译 wasmtime，几分钟、几 GB；磁盘紧张时把它们指到别的盘。
set -euo pipefail
cd "$(dirname "$0")"

GUEST_TARGET="${E2E_GUEST_TARGET:-$PWD/target/guest}"
HOST_TARGET="${E2E_HOST_TARGET:-$PWD/target/host}"
export CARGO_INCREMENTAL=0

for guest in guest-ri guest-base; do
  (cd "$guest" && CARGO_TARGET_DIR="$GUEST_TARGET" cargo build --release --target wasm32-wasip2)
done

# 宿主侧 bindgen 用的世界 `harness` 不属于 WIT 仓库，放在这里，每次拷一份 WIT 进来再加上它
rm -rf host/wit
cp -R ../wit host/wit
cp host/harness.wit host/wit/harness.wit

(cd host && CARGO_TARGET_DIR="$HOST_TARGET" cargo build)
"$HOST_TARGET/debug/e2e-host" "$GUEST_TARGET/wasm32-wasip2/release"
