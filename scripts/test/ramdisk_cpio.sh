#!/bin/bash
set -e

print_info() {
    printf "\033[1;37m%s\033[0m" "[RustSBI-Arceboot Build For Test] "
    printf "\033[1;32m%s\033[0m" "[INFO] "
    printf "\033[36m%s\033[0m\n" "$1"
}

print_info "开始执行 ramdisk 类型的 cpio 打包脚本"
print_info "此为空镜像, 只含有一个 arceboot.txt 文件, 用于测试 Arceboot"
print_info "即将在当前目录执行创建 -------->"

mkdir myramdisk
touch myramdisk/arceboot.txt
echo "This is a test file for Arceboot." > myramdisk/arceboot.txt

cd myramdisk
find . | cpio -o --format=newc > ../ramdisk.cpio
cd ..

rm -rf myramdisk

print_info "创建完成, 生成的 ramdisk.cpio 位于当前目录"