#!/bin/bash
set -e

PROJECT_ROOT=$(pwd)
IMG_NAME="fat32_disk_test.img"
MOUNT_DIR="mnt_fat32"
ESP_DIR="$MOUNT_DIR/EFI/BOOT"

if [ ! -d "$MOUNT_DIR" ]; then
    mkdir "$MOUNT_DIR"
fi

echo "[1/3] 挂载 FAT32 镜像..."
sudo mount -o loop "$IMG_NAME" "$MOUNT_DIR"

echo "[2/3] 创建 ESP 目录结构..."
sudo mkdir -p "$ESP_DIR"

echo "[3/3] 复制 efi 文件到 ESP..."
sudo cp "$PROJECT_ROOT/edk2/Build/DEBUG_GCC5/RISCV64/HelloRiscv.efi" "$ESP_DIR/BOOTRISCV64.EFI"

sudo find "$ESP_DIR" -type d | while read -r dir; do
    echo "$dir"
done

sudo umount "$MOUNT_DIR"