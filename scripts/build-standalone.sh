#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="${1:-dist-standalone}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "=========================================================="
echo "  My Computer Keeper - Standalone Executable Builder"
echo "=========================================================="

echo "[1/3] 建置前端桌面資源 (React + TypeScript + Vite)..."
cd "$ROOT_DIR/ui"
if [ ! -d "node_modules" ]; then
    npm install
fi
npm run build

echo "[2/3] 編譯 Rust 核心引擎與內嵌單獨執行檔 (Release 模式)..."
cd "$ROOT_DIR"
cargo build --release -p my-computer-keeper

echo "[3/3] 輸出單獨執行檔..."
mkdir -p "$ROOT_DIR/$OUT_DIR"
BIN_SRC="$ROOT_DIR/target/release/my-computer-keeper"
if [ -f "${BIN_SRC}.exe" ]; then
    cp -f "${BIN_SRC}.exe" "$ROOT_DIR/$OUT_DIR/"
    DEST_EXE="$ROOT_DIR/$OUT_DIR/my-computer-keeper.exe"
else
    cp -f "$BIN_SRC" "$ROOT_DIR/$OUT_DIR/"
    DEST_EXE="$ROOT_DIR/$OUT_DIR/my-computer-keeper"
fi

echo "----------------------------------------------------------"
echo " [V] 單獨執行檔建置完成！"
echo " 輸出路徑 : $DEST_EXE"
echo " 特色說明 : 前端 UI 與系統核心已 100% 內嵌，單檔即可在任何目錄獨立執行！"
echo "----------------------------------------------------------"
