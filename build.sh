#!/bin/bash
set -e

# 预定义变量与函数
TARGET=riscv64gc-unknown-none-elf
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ELF=$SCRIPT_DIR/target/$TARGET/release/arceboot
BIN=$SCRIPT_DIR/target/$TARGET/release/arceboot.bin
SBI=$SCRIPT_DIR/rustsbi/target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper-payload.elf

print_info() {
    printf "\033[1;37m%s\033[0m" "[RustSBI-Arceboot Build] "
    printf "\033[1;32m%s\033[0m" "[INFO] "
    printf "\033[36m%s\033[0m\n" "$1"
}

# 编译
print_info "开始编译 ArceBoot..."
cargo rustc --release --target $TARGET -- \
    -C opt-level=z \
    -C panic=abort \
    -C relocation-model=static \
    -C target-cpu=generic \
    -C link-args="-Tlink.ld"
print_info "编译 ArceBoot 成功"

# 提取 binary
print_info "开始提取 ArceBoot Binary..."
rust-objcopy --binary-architecture=riscv64 $ELF --strip-all -O binary $BIN
print_info "提取 ArceBoot Binary 成功"

# 检查并克隆 rustsbi 仓库
# TODO: 后面合入 rustsbi 后这段需要改
if [ ! -d "rustsbi" ]; then
    print_info "仓库 rustsbi 不存在，正在克隆仓库..."
    git clone https://github.com/rustsbi/rustsbi.git
    print_info "仓库 rustsbi 克隆仓库成功"
else
    print_info "仓库 rustsbi 已存在"
fi

# 编译 rustsbi
print_info "开始以 payload 形式编译 rustsbi..."
cd rustsbi && cargo prototyper --payload $BIN
print_info "rustsbi 编译成功"

print_info "所有任务执行完成,项目构建成功,目标 elf 文件位于: $SBI"