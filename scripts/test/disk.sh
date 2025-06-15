#!/bin/bash
set -e

IMG_NAME="fat32_disk_test.img"
IMG_SIZE_MB=512
MOUNT_DIR="mnt_fat32"

echo "[1/2] 创建空镜像文件..."
dd if=/dev/zero of=$IMG_NAME bs=1M count=$IMG_SIZE_MB

echo "[2/2] 格式化为 FAT32 文件系统..."
mkfs.vfat -F 32 $IMG_NAME

echo "======== DONE ========"