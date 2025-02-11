# 使用 OpenSBI & EDK2 在 QEMU 中启动 Ubuntu 24.04.1

本教程给出了使用 OpenSBI 和 EDK II 在 QEMU 中启动 Ubuntu 24.04.1 的基本流程。

请读者在其主机上安装必要的软件来尝试本教程。本教程是在 Arch Linux 上开发的，建议读者使用 x86_64 平台上的 Linux 环境按照本教程进行尝试。

本教程使用软件版本如下：

|         软件          |     版本     |
| :-------------------: | :----------: |
| riscv64-linux-gnu-gcc |    14.2.0    |
|  qemu-system-riscv64  |     9.2.0    |
|       OpenSBI         |      1.6     |
|        EDK II         | stable202411 |

## 准备 Opensbi，EDK II，Ubuntu 24.04.1
创建工作目录并进入该目录

``` shell
$ mkdir workshop && cd workshop
```

### Clone Opensbi

``` shell
$ git clone -b v1.6 https://github.com/riscv/opensbi.git
```

### Clone EDK II

``` shell
$ git clone -b edk2-stable202411 --recurse-submodule git@github.com:tianocore/edk2.git
```

### 下载 Ubuntu 24.04.1 镜像文件

下载链接：[Ubuntu 24.04.1](https://cdimage.ubuntu.com/releases/24.04.1/release/ubuntu-24.04.1-preinstalled-server-riscv64.img.xz)
``` shell
$ wget https://cdimage.ubuntu.com/releases/24.04.1/release/ubuntu-24.04.1-preinstalled-server-riscv64.img.xz
$ xz -d ubuntu-24.04.1-preinstalled-server-riscv64.img.xz
```

- The password of the default user `ubuntu` is `ubuntu`.
- 登录后应会被要求更改登录密码。
- 可以通过 `sudo` 更改 root 密码。

## 编译 EDK II

设置环境变量

``` shell
$ export WORKSPACE=`pwd`
$ export GCC5_RISCV64_PREFIX=riscv64-linux-gnu-
$ export PACKAGES_PATH=$WORKSPACE/edk2
$ export EDK_TOOLS_PATH=$WORKSPACE/edk2/BaseTools
$ source edk2/edksetup.sh --reconfig
```

编译 BaseTools

``` shell
$ make -C edk2/BaseTools
```

编译 RiscVVirtQemu

``` shell
$ source edk2/edksetup.sh BaseTools
$ build -a RISCV64 --buildtarget RELEASE -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC5
```

## 编译 OpenSBI

``` shell
$ make -C opensbi \
    -j $(nproc) \
    CROSS_COMPILE=riscv64-linux-gnu- \
    PLATFORM=generic
```

## 使用 OpenSBI 和 EDK II 启动 Ubuntu 24.04.1

将 RISCV_VIRT_CODE.fd 和 RISCV_VIRT_VARS.fd 填充至 32M，以适应 RISC-V QEMU pflash devices 的需求

``` shell
$ truncate -s 32M Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_CODE.fd
$ truncate -s 32M Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_VARS.fd
```

启动 qemu-system-riscv64

``` shell
$ qemu-system-riscv64  \
    -M virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
    -m 4096 -smp 2  -nographic \
    -bios opensbi/build/platform/generic/firmware/fw_dynamic.bin \
    -blockdev node-name=pflash0,driver=file,read-only=on,filename=Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_CODE.fd  \
    -blockdev node-name=pflash1,driver=file,filename=Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_VARS.fd \
    -device virtio-blk-device,drive=hd0  \
    -drive file=ubuntu-24.04.1-preinstalled-server-riscv64.img,format=raw,id=hd0,if=none
```