# 使用RustSBI & U-Boot在QEMU中启动Linux内核

本教程给出了使用RustSBI和U-Boot在QEMU中启动Linux内核的基本流程。高级用户可以在本教程中配置或构建各种内容时尝试不同的选项。

请读者在其主机上安装必要的软件来尝试本教程的脚本。本教程是在Arch Linux上开发的。

[环境配置](#环境配置)小节给出了本教程的环境配置方法，用户在使用本教程时需要先完成环境配置小节内容。

[编译Linux Kernel](#编译linux-kernel)小节给出了Linux Kernel的编译流程，并使用编译好的Linux Kernel镜像制作启动盘。

RustSBI 原型系统提供动态固件，根据前一个阶段传入的信息动态加载下一个阶段。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
| riscv64-linux-gnu-gcc | 14.1.0  |
|  qemu-system-riscv64  |  9.0.1  |
|  RustSBI Prototyper   |  0.0.0  |
|        U-Boot         | 2024.04 |
|     Linux Kernel      |   6.2   |
|        busybox        | 1.36.0  |

## 环境配置

### 安装交叉编译器和QEMU

For Arch Linux:

```shell
$ sudo pacman -S git riscv64-linux-gnu-gcc qemu-system-riscv
```

For Ubuntu:

```shell
$ sudo apt-get update && sudo apt-get upgrade
$ sudo apt-get install git qemu-system-misc gcc-riscv64-linux-gnu
```

#### 测试是否成功安装

For riscv64-linux-gnu-gcc:

```shell
$ riscv64-linux-gnu-gcc --version
```

它将输出以下版本信息

```
riscv64-linux-gnu-gcc (GCC) 14.1.0
Copyright (C) 2024 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
```

For QEMU:

```shell
$ qemu-system-riscv64 --version
```

它将输出以下版本信息

```
QEMU emulator version 9.0.1
Copyright (c) 2003-2024 Fabrice Bellard and the QEMU Project developers
```

### 准备RustSBI Prototyper， U-Boot ， busybox和Linux Kernel源码

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

Clone RustSBI Prototyper

```shell
$ git clone -b main https://github.com/rustsbi/rustsbi.git
```

Clone U-Boot

```shell
$ git clone -b v2024.04 https://github.com/u-boot/u-boot.git
```

Clone busybox

```shell
$ git clone -b 1_36_0 https://github.com/mirror/busybox.git
```

Clone Linux Kernel

```shell
$ git clone -b v6.2 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git
```

## 编译Linux Kernel

进入`linux`目录

```shell
$ cd linux
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
```

生成`.config`文件

```shell
$ make defconfig
```

验证`.config`文件是否存在RISC-V

```shell
$ grep --color=always -ni 'riscv' .config
```

观察到RISC-V 配置选项已启用

```
CONFIG_RISCV=y
```

编译Linux Kernel

```shell
$ make -j$(nproc)
```

生成的文件`Image` 和 `Image.gz` 可以在`arch/riscv/boot/`目录找到。 `Image.gz`是 `Image` 的压缩形式。

### 创建根文件系统

#### 编译busybox

> busybox在Ubuntu 22.04和Arch Linux系统上编译时会报错，推荐在Ubuntu 20.04系统上编译。

进入busybox目录

```shell
$ cd busybox
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
```

编译busybox

```shell
$ make defconfig
$ make menuconfig
# Enable the Build static binary (no shared libs) option in Settings-->Build Options
$ make -j $(nproc)
$ make install
```

#### 创建启动盘

在`workshop`目录运行以下命令来创建一个1 GB的磁盘镜像

```shell
# Create a 1 GB disk image
$ qemu-img create linux-rootfs.img 1g
```

#### 创建分区

将在磁盘映像`linux-rootfs.img`上创建1个分区，这个分区是可引导的。

`parted`命令将用于在镜像`linux-rootfs.img`中创建分区。在镜像中创建分区表：

```shell
$ sudo parted linux-rootfs.img mklabel gpt
```

现在`linux-rootfs.img`中有一个分区表。将`linux-rootfs.img`挂载为loop device，以便它可以用作块设备。将`linux-rootfs.img`挂载为块设备将允许在其中创建分区。

```shell
# Attach linux-rootfs.img with the first available loop device
$ sudo losetup --find --show linux-rootfs.img
```

> - `find`：查找第一个未使用的loop device
> - `show`：显示`linux-rootfs.img`附加到的loop device的名称

记下循环设备的完整路径。在本教程中它是`/dev/loop0`。对`/dev/loop0`的操作将会对`linux-rootfs.img`进行操作。

对`/dev/loop0`分区

```shell
# Create a couple of primary partitions
$ sudo parted --align minimal /dev/loop0 mkpart primary ext4 0 100%

$ sudo parted /dev/loop0 print
```

#### 格式化分区

通过以下命令查看分区：

```shell
$ ls -l /dev/loop0*
```

在本教程中，分区为`/dev/loop0p1`。

格式化分区并创建`ext4`文件系统，同时将分区设置为可引导分区。

```shell
$ sudo mkfs.ext4 /dev/loop0p1

# Mark first partition as bootable
$ sudo parted /dev/loop0 set 1 boot on
```

#### 将Linux Kernel和根文件系统拷贝进启动盘

```shell
# Mount the 1st partition
$ sudo mkdir rootfs
$ sudo mount /dev/loop0p1 rootfs
$ cd rootfs
```

拷贝Linux Kernel镜像

```shell
$ sudo cp ../linux/arch/riscv/boot/Image .
```

拷贝根文件系统

```shell
$ sudo cp -r ../busybox/_install/* .
$ sudo mkdir proc sys dev etc etc/init.d
$ cd etc/init.d/
$ sudo cat > rcS << EOF
  #!/bin/sh
  mount -t proc none /proc
  mount -t sysfs none /sys
  /sbin/mdev -s
  EOF
$ sudo chmod +x rcS
```

卸载`rootfs`

```shell
$ cd workshop
$ sudo umount rootfs
```

将`/dev/loop0`分离

```shell
$ sudo losetup -d /dev/loop0
```

## 编译RustSBI Prototyper

进入rustsbi目录

```shell
$ cd rustsbi
```

编译RustSBI Prototyper

```shell
$ cargo prototyper
```

## 编译U-Boot SPL

进入U-Boot目录

```shell
$ cd u-boot
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
$ export OPENSBI=../rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin
```

生成`.config`文件

```shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
# add bootcmd value
$ make menuconfig
```

U-Boot 配置选项将加载到终端。导航到 `Boot options` $\rightarrow$ `bootcmd value` 并将以下内容写入 `bootcmd` 值：

```
ext4load virtio 0:1 84000000 Image; setenv bootargs root=/dev/vda1 rw console=ttyS0; booti 0x84000000 - ${fdtcontroladdr}
```

编译U-Boot

```shell
# To build U-Boot
$ make -j$(nproc)
```

本小节将使用二进制文件 `./spl/u-boot-spl`和`./u-boot.itb `。

## 使用RustSBI 原型系统和U-Boot启动Linux Kernel

进入`workshop`目录

```shell
$ cd workshop
```

运行下面命令

```shell
$ qemu-system-riscv64 -M virt -smp 1 -m 256M -nographic \
          -bios ./u-boot/spl/u-boot-spl \
          -device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
          -blockdev driver=file,filename=./linux-rootfs.img,node-name=hd0 \
          -device virtio-blk-device,drive=hd0
```
