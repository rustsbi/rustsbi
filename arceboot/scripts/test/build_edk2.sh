#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ARCEBOOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

cd "$ARCEBOOT_DIR"

WORKSPACE_DIR="${ARCEBOOT_DIR}/edk2"
EDK_DIR="$WORKSPACE_DIR/edk2"
CONF_DIR="$EDK_DIR/Conf"

mkdir -p "$WORKSPACE_DIR"

echo "[1/4] 准备 EDK2 构建环境..."
cd "$WORKSPACE_DIR"

if [ ! -d "ToolChain/RISCV" ]; then
    echo "下载 RISCV 工具链..."
    mkdir -p ToolChain/RISCV
    wget -q https://github.com/riscv-collab/riscv-gnu-toolchain/releases/download/2025.07.16/riscv64-elf-ubuntu-24.04-gcc-nightly-2025.07.16-nightly.tar.xz
    tar -xf riscv64-elf-ubuntu-24.04-gcc-nightly-2025.07.16-nightly.tar.xz -C "${WORKSPACE_DIR}/ToolChain/RISCV"
else
    echo "RISCV 工具链已存在，跳过下载。"
fi

echo "[2/4] 克隆 EDK2 仓库..."
if [ ! -d "$EDK_DIR" ]; then
    git clone --recurse-submodule https://github.com/tianocore/edk2.git "$EDK_DIR"
else
    echo "EDK2 仓库已存在，跳过克隆。"
fi

export PATH="$WORKSPACE_DIR/ToolChain/RISCV/riscv/bin:$PATH"

echo "[3/4] 构建 EDK2..."
cd "$WORKSPACE_DIR"

export WORKSPACE="$WORKSPACE_DIR"
export GCC_RISCV64_PREFIX=riscv64-unknown-elf-
export GCC5_RISCV64_PREFIX=riscv64-unknown-elf-
export PACKAGES_PATH="$EDK_DIR"
export EDK_TOOLS_PATH="$EDK_DIR/BaseTools"
export CONF_PATH="$CONF_DIR"
export PYTHON_COMMAND="${PYTHON_COMMAND:-python3}"
export EDK2_TOOLCHAIN_TAG="${EDK2_TOOLCHAIN_TAG:-GCC}"

source_edksetup() {
    set +u
    . "$EDK_DIR/edksetup.sh" "$@"
    set -u
}

build_example() {
    local dsc_path=$1
    build -a RISCV64 -t "$EDK2_TOOLCHAIN_TAG" -p "$dsc_path"
}

source_edksetup --reconfig
make -C "$EDK_DIR/BaseTools"

mkdir -p "$CONF_PATH"
cp -f "$EDK_DIR/BaseTools/Conf/target.template" "$CONF_PATH/target.txt"
cp -f "$EDK_DIR/BaseTools/Conf/tools_def.template" "$CONF_PATH/tools_def.txt"
cp -f "$EDK_DIR/BaseTools/Conf/build_rule.template" "$CONF_PATH/build_rule.txt"

source_edksetup BaseTools
export CONF_PATH="$CONF_DIR"

echo "[4/4] 准备 HelloRiscv 和 AllocatePage 示例..."
# edk2-Hello
mkdir -p "$EDK_DIR/Hello"
cp -r "$ARCEBOOT_DIR/tests/edk2-Hello" "$EDK_DIR"
mv "$EDK_DIR/edk2-Hello"/* "$EDK_DIR/Hello/"
cp -r "$EDK_DIR/MdeModulePkg/MdeModulePkg.dsc" "$EDK_DIR/Hello/Hello.dsc"
printf "\n[Components]\n  Hello/Hello.inf\n" >> "$EDK_DIR/Hello/Hello.dsc"
build_example "$EDK_DIR/Hello/Hello.dsc"

# edk2-HelloRiscv
cp -r "$ARCEBOOT_DIR/tests/edk2-HelloRiscv" "$EDK_DIR"
mv "$EDK_DIR/edk2-HelloRiscv" "$EDK_DIR/HelloRiscv/"
build_example "$EDK_DIR/HelloRiscv/HelloRiscv.dsc"

# edk2-AllocatePage
cp -r "$ARCEBOOT_DIR/tests/edk2-AllocatePage" "$EDK_DIR"
mv "$EDK_DIR/edk2-AllocatePage" "$EDK_DIR/AllocatePage/"
build_example "$EDK_DIR/AllocatePage/AllocatePage.dsc"

echo "EDK2 与示例构建完成。"
