# Linux 内核（不使用发行版）

[Linux](https://www.kernel.org/) 是开源的操作系统内核，支持 RISC-V 等处理器架构。

本节使用 BusyBox 准备根文件系统，以 QEMU `virt` 平台为例，介绍直接使用 RustSBI，以及使用 RustSBI 和 U-Boot 启动 Linux 内核的方法。

## 在 QEMU 中启动 Linux 内核

本教程给出了在 QEMU 中启动 Linux 内核的基本流程，分为两种类型：

1. 直接使用 RustSBI 启动 Linux 内核。
2. 使用 RustSBI 和 U-Boot 启动 Linux 内核。

高级用户可以在配置或构建时尝试不同的选项。

本教程在 Arch Linux 或 Ubuntu 的 Bash 终端中操作。开始前，请安装 Rustup。

[环境配置](#环境配置)小节给出了本教程的环境配置方法，请先完成该小节的内容。

[编译 Linux 内核](#编译-linux-内核)小节给出了内核的编译流程，并使用编译好的内核镜像制作启动盘。

RustSBI Prototyper 提供动态固件，根据上一引导阶段传入的信息确定下一阶段的入口并跳转。

本教程使用软件版本如下：

|         软件         |             版本             |
| :------------------: | :--------------------------: |
| riscv64-linux-gnu-gcc |            14.1.0            |
| qemu-system-riscv64   |             9.0.1            |
| RustSBI Prototyper    |             0.0.0            |
| U-Boot               |            2024.04           |
| Linux                |           6.12.110           |
| BusyBox              |             1.36.1           |

### 环境配置

#### 安装交叉编译器、QEMU 和构建工具

在 Arch Linux 中安装：

```shell
$ sudo pacman -S git base-devel riscv64-linux-gnu-gcc qemu-system-riscv qemu-img parted e2fsprogs ncurses openssl
```

在 Ubuntu 中安装：

```shell
$ sudo apt update
$ sudo apt install git build-essential gcc-riscv64-linux-gnu libc6-dev-riscv64-cross qemu-system-misc qemu-utils bison flex bc libncurses-dev libssl-dev parted e2fsprogs
```

##### 测试是否成功安装

检查交叉编译器版本：

```shell
$ riscv64-linux-gnu-gcc --version
```

输出示例：

```
riscv64-linux-gnu-gcc (GCC) 14.1.0
Copyright (C) 2024 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
```

检查 QEMU 版本：

```shell
$ qemu-system-riscv64 --version
```

输出示例：

```
QEMU emulator version 9.0.1
Copyright (c) 2003-2024 Fabrice Bellard and the QEMU Project developers
```

#### 准备 RustSBI Prototyper、U-Boot、BusyBox 和 Linux 内核源码

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

Clone RustSBI

```shell
$ git clone -b main https://github.com/rustsbi/rustsbi.git
```

Clone BusyBox

```shell
$ git clone -b 1_36_1 https://git.busybox.net/busybox/
```

Clone Linux 内核

```shell
$ git clone --depth 1 -b v6.12.110 https://git.kernel.org/pub/scm/linux/kernel/git/stable/linux.git
```

##### Clone U-Boot

仅在使用 RustSBI 和 U-Boot 启动 Linux 内核时需要安装以下额外依赖并下载 U-Boot。若直接使用 RustSBI 启动，可跳过本小节及后续使用 U-Boot 启动的小节。

在 Arch Linux 中安装：

```shell
$ sudo pacman -S gnutls python python-setuptools python-pyelftools swig
```

在 Ubuntu 中安装：

```shell
$ sudo apt install libgnutls28-dev python3-dev python3-setuptools python3-pyelftools swig
```

下载 U-Boot：

```shell
$ git clone -b v2024.04 https://github.com/u-boot/u-boot.git
```

### 编译 Linux 内核

进入 `linux` 目录

```shell
$ cd linux
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
```

生成 `.config` 文件

```shell
$ make defconfig
```

检查 `.config` 文件中的 RISC-V 配置

```shell
$ grep --color=always -ni 'riscv' .config
```

确认 RISC-V 配置选项已启用

```
CONFIG_RISCV=y
```

编译 Linux 内核

```shell
$ make -j$(nproc) Image
```

生成的内核镜像 `Image` 位于 `arch/riscv/boot/` 目录。

#### 创建根文件系统

##### 编译 BusyBox

BusyBox 使用静态链接，无需向根文件系统复制动态库。`tc` 工具与较新的 Linux 用户态头文件不兼容，本教程在默认配置基础上关闭该工具。

进入 BusyBox 目录

```shell
$ cd ../busybox
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
```

编译 BusyBox

```shell
$ make defconfig
$ sed -i -e 's/# CONFIG_STATIC is not set/CONFIG_STATIC=y/' -e 's/CONFIG_TC=y/# CONFIG_TC is not set/' .config
$ make -j$(nproc)
$ make install
```

##### 创建启动盘

返回 `workshop` 目录，创建一个 1 GiB 的磁盘镜像

```shell
$ cd ..
# Create a 1 GiB disk image
$ qemu-img create linux-rootfs.img 1g
```

##### 创建分区

在磁盘镜像 `linux-rootfs.img` 中创建一个 ext4 分区，用于存放内核和根文件系统。

使用 `parted` 在镜像中创建分区表：

```shell
$ sudo parted --script linux-rootfs.img mklabel gpt
```

将 `linux-rootfs.img` 关联到空闲的循环设备，并将设备路径保存在 `loop_device` 中：

```shell
# Attach linux-rootfs.img with the first available loop device
$ loop_device=$(sudo losetup --find --show --partscan linux-rootfs.img)
$ echo "$loop_device"
```

> - `--find`：查找第一个未使用的循环设备。
> - `--show`：输出循环设备的路径。
> - `--partscan`：扫描分区表，创建分区设备。

后续命令使用 `loop_device`，其值可能为 `/dev/loop0` 或其他空闲设备。

在循环设备上创建分区

```shell
# Create an ext4 partition
$ sudo parted --script --align optimal "$loop_device" mkpart primary ext4 1MiB 100%
$ sudo partprobe "$loop_device"
$ sudo parted "$loop_device" print
```

##### 格式化分区

通过以下命令查看分区：

```shell
$ ls -l "${loop_device}"*
```

第一个分区的设备路径为 `${loop_device}p1`。

格式化分区，创建 ext4 文件系统。

```shell
$ sudo mkfs.ext4 "${loop_device}p1"
```

##### 将 Linux 内核和根文件系统复制到启动盘

```shell
# Mount the 1st partition
$ mkdir rootfs
$ sudo mount "${loop_device}p1" rootfs
$ cd rootfs
```

复制 Linux 内核镜像

```shell
$ sudo cp ../linux/arch/riscv/boot/Image .
```

复制根文件系统

```shell
$ sudo cp -a ../busybox/_install/. .
$ sudo mkdir -p proc sys dev etc/init.d
$ cd etc/init.d/
$ sudo tee rcS > /dev/null <<'EOF'
#!/bin/sh
mount -t proc none /proc
mount -t sysfs none /sys
/sbin/mdev -s
EOF
$ sudo chmod +x rcS
```

返回 `workshop` 目录，卸载根文件系统

```shell
$ cd ../../..
$ sudo umount rootfs
```

分离循环设备

```shell
$ sudo losetup -d "$loop_device"
```

### 编译 RustSBI Prototyper

进入 RustSBI 目录

```shell
$ cd rustsbi
```

Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。安装构建工具，编译 RustSBI Prototyper，完成后返回 `workshop` 目录

```shell
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
$ cargo prototyper build
$ cd ..
```

后续两种启动流程使用相同的 Linux 内核和根文件系统镜像。

### 直接使用 RustSBI 启动 Linux 内核

在 `workshop` 目录运行下面命令：

```shell
$ qemu-system-riscv64 \
-machine virt -nographic -m 256M -smp 1 \
-bios ./rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
-kernel ./linux/arch/riscv/boot/Image \
-append 'root=/dev/vda1 rootwait rw console=ttyS0' \
-blockdev driver=file,filename=./linux-rootfs.img,node-name=hd0 \
-device virtio-blk-device,drive=hd0
```

QEMU 加载 Linux 内核，并通过设备树传递内核参数。RustSBI 动态固件跳转到内核入口后，Linux 将磁盘镜像的第一个分区挂载为根文件系统。

启动后，BusyBox 的 `init` 会执行 `/etc/init.d/rcS`，挂载 proc 和 sysfs 文件系统，并启动交互式终端。

### 使用 RustSBI 和 U-Boot 启动 Linux 内核

本小节在 `workshop` 目录下操作，使用前面生成的动态固件、Linux 内核和根文件系统镜像。

#### 编译 U-Boot SPL

进入 U-Boot 目录

```shell
$ cd u-boot
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
$ export OPENSBI=../rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin
```

生成 `.config` 文件

```shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
# add bootcmd value
$ make menuconfig
```

U-Boot 配置选项将加载到终端。选择 `Boot options` → `bootcmd value` 并将以下内容写入 `bootcmd` 值：

```
ext4load virtio 0:1 84000000 Image; setenv bootargs root=/dev/vda1 rootwait rw console=ttyS0; booti 0x84000000 - ${fdtcontroladdr}
```

编译 U-Boot

```shell
# To build U-Boot
$ make -j$(nproc)
```

本小节使用二进制文件 `spl/u-boot-spl` 和 `u-boot.itb`。RustSBI 动态固件通过 `OPENSBI` 打包到 `u-boot.itb`，由 U-Boot SPL 加载。

#### 启动 Linux 内核

返回 `workshop` 目录

```shell
$ cd ..
```

运行下面命令

```shell
$ qemu-system-riscv64 -M virt -smp 1 -m 256M -nographic \
          -bios ./u-boot/spl/u-boot-spl \
          -device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
          -blockdev driver=file,filename=./linux-rootfs.img,node-name=hd0 \
          -device virtio-blk-device,drive=hd0
```

启动后，BusyBox 的 `init` 会执行 `/etc/init.d/rcS`，挂载 proc 和 sysfs 文件系统，并启动交互式终端。
