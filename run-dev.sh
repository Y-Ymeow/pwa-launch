#!/bin/bash
# 禁用 WebKitGTK 硬件加速，防止内存泄漏
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_WEBGL=1
export WEBKIT_DISABLE_ACCELERATED_2D_CANVAS=1

bunx tauri dev
