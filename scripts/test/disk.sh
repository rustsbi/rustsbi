#!/bin/bash
set -e

print_info() {
    printf "\033[1;37m%s\033[0m" "[RustSBI-Arceboot Build For Test] "
    printf "\033[1;32m%s\033[0m" "[INFO] "
    printf "\033[36m%s\033[0m\n" "$1"
}

print_info "开始执行 virtio-block 类型的 disk 创建脚本"
print_info "此为 FAT32 文件系统镜像, 只含有一个 arceboot.txt 文件, 用于测试 Arceboot"
print_info "即将在当前目录执行创建 -------->"

dd if=/dev/zero of=disk.img bs=1M count=512
mkfs.vfat -F 32 disk.img

mkdir temp
sudo mount -o loop disk.img temp
mkdir -p temp/test
touch temp/test/arceboot.txt
echo "This is a test file for Arceboot." > temp/test/arceboot.txt
sudo umount temp
rm -rf temp

print_info "创建完成, 生成的 disk.img 位于当前目录"