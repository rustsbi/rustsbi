#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ARCEBOOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
REPO_ROOT="$(cd "$ARCEBOOT_DIR/.." && pwd)"

cd "$ARCEBOOT_DIR"

IMG_NAME="disk.img"
MOUNT_DIR="mnt_fat32"
ESP_DIR="$MOUNT_DIR/EFI/BOOT"

EFI_FILE="${EFI_FILE:-}"

if [ -z "$EFI_FILE" ]; then
    EFI_FILE="$(find "$ARCEBOOT_DIR/edk2/Build" -type f -name BOOTRISCV64.EFI -print -quit)"
fi

# EFI_FILE 可以是绝对路径、相对于 ARCEBOOT_DIR 的路径，
# 或者相对于仓库根目录的路径。
if [ "${EFI_FILE#/}" != "$EFI_FILE" ]; then
    SRC_EFI="$EFI_FILE"
elif [ -f "$ARCEBOOT_DIR/$EFI_FILE" ]; then
    SRC_EFI="$ARCEBOOT_DIR/$EFI_FILE"
elif [ -f "$REPO_ROOT/$EFI_FILE" ]; then
    SRC_EFI="$REPO_ROOT/$EFI_FILE"
else
    echo "EFI file not found: $EFI_FILE" >&2
    exit 1
fi

if [ ! -d "$MOUNT_DIR" ]; then
    mkdir "$MOUNT_DIR"
fi

echo "[1/3] 挂载 FAT32 镜像..."
sudo mount -o loop "$IMG_NAME" "$MOUNT_DIR"

echo "[2/3] 创建 ESP 目录结构..."
sudo mkdir -p "$ESP_DIR"

echo "[3/3] 复制 efi 文件到 ESP..."
echo "源文件: $SRC_EFI"
sudo cp "$SRC_EFI" "$ESP_DIR/BOOTRISCV64.EFI"

sudo find "$ESP_DIR" -type d | while read -r dir; do
    echo "$dir"
done

sudo umount "$MOUNT_DIR"
