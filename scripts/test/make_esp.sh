#!/bin/bash
set -e

PROJECT_ROOT=$(pwd)
IMG_NAME="disk.img"
MOUNT_DIR="mnt_fat32"
ESP_DIR="$MOUNT_DIR/EFI/BOOT"
EFI_NAME="${EFI_NAME:-HelloRiscv}"
BUILD_FLAVOR="${BUILD_FLAVOR:-DEBUG_GCC5}"
ARCH_DIR="${ARCH_DIR:-RISCV64}"

if [ ! -d "$MOUNT_DIR" ]; then
    mkdir "$MOUNT_DIR"
fi

echo "[1/3] 挂载 FAT32 镜像..."
sudo mount -o loop "$IMG_NAME" "$MOUNT_DIR"

echo "[2/3] 创建 ESP 目录结构..."
sudo mkdir -p "$ESP_DIR"

echo "[3/3] 复制 efi 文件到 ESP..."
SRC_EFI="$PROJECT_ROOT/edk2/Build/$BUILD_FLAVOR/$ARCH_DIR/${EFI_NAME}.efi"
echo "源文件: $SRC_EFI"
sudo cp "$SRC_EFI" "$ESP_DIR/BOOTRISCV64.EFI"

sudo find "$ESP_DIR" -type d | while read -r dir; do
    echo "$dir"
done

sudo umount "$MOUNT_DIR"