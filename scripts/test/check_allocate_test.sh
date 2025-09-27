#!/bin/bash
set -e

LOG_FILE="qemu.log"
TARGET_STRING="EFI Output: 0xDEADBEEF12345678"

if [ ! -f "$LOG_FILE" ]; then
    echo "❌ $LOG_FILE 不存在"
    exit 1
fi

if grep -qF "$TARGET_STRING" "$LOG_FILE"; then
    echo "✅ 找到匹配日志行"
    exit 0
else
    echo "❌ 未找到匹配日志行"
    exit 2
fi