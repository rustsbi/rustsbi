# 使用RustSBI & U-Boot SPL在QEMU中启动Test Kernel

本教程给出了使用RustSBI和U-Boot在QEMU中启动Test Kernel的基本流程。

其中启动流程分为两种类型：
1. 只使用U-Boot SPL的启动流程
2. 同时使用U-Boot SPL和U-Boot的启动流程。

请读者在其主机上安装必要的软件来尝试本教程的脚本。本教程是在Arch Linux上开发的。

[环境配置](#环境配置)小节给出了本教程的环境配置方法，用户在使用本教程时需要先完成环境配置小节内容。

[使用U-Boot SPL启动Test Kerenl](#使用U-Boot-SPL启动Test-Kerenl)小节给出了只使用U-Boot SPL的启动流程。

[使用U-Boot SPL和U-Boot启动Test Kerenl](#使用U-Boot-SPL和U-Boot启动Test-Kerenl)小节给出了同时使用U-Boot SPL和U-Boot的启动流程。

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

### 安装交叉编译器、QEMU和相关依赖

For Arhc Linux:

``` shell
$ sudo pacman -S git riscv64-linux-gnu-gcc qemu-system-riscv uboot-tools
```

For Ubuntu:

``` shell
$ sudo apt-get update && sudo apt-get upgrade
$ sudo apt-get install git qemu-system-misc gcc-riscv64-linux-gnu u-boot-tools
```

#### 测试是否成功安装

For riscv64-linux-gnu-gcc:

``` shell
$ riscv64-linux-gnu-gcc --version
```

它将输出以下版本信息

``` 
riscv64-linux-gnu-gcc (GCC) 14.1.0
Copyright (C) 2024 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
```

For QEMU:

``` shell
$ qemu-system-riscv64 --version
```

它将输出以下版本信息

``` 
QEMU emulator version 9.0.1
Copyright (c) 2003-2024 Fabrice Bellard and the QEMU Project developers
```

### 准备RustSBI Prototyper, Test Kernel, U-Boot源码

创建工作目录并进入该目录

``` shell
$ mkdir workshop && cd workshop
```

Clone RustSBI Prototyper

``` shell
$ git clone https://github.com/rustsbi/prototyper.git && cd prototyper && git checkout main && cd ..
```

Clone U-Boot

``` shell
$ git clone https://github.com/u-boot/u-boot.git && cd u-boot && git checkout v2024.04 && cd ..
```

## 使用U-Boot SPL启动Test Kerenl
### 编译RustSBI  Prototyper和Test Kernel

进入prototyper目录

``` shell
$ cd prototyper
```

编译RustSBI Prototyper和Test Kernel

``` shell
$ cargo make test-kernel-itb
```

本小节将使用二进制文件 `./target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.itb`。

### 编译U-Boot SPL

进入U-Boot目录

``` shell
$ cd u-boot
```

导出环境变量

``` shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
$ export OPENSBI=../prototyper/target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper.bin 
```

生成`.config`文件

``` shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
# add bootcmd value
$ make menuconfig
```

编译U-Boot

``` shell
# To build U-Boot
$ make -j$(nproc)
```

本小节将使用二进制文件 `./spl/u-boot-spl`。

### 使用RustSBI原型系统和U-Boot启动Linux Kernel

进入`workshop`目录

``` shell
$ cd workshop
```

运行下面命令

``` shell
$ qemu-system-riscv64 -M virt -smp 1 -m 256M -nographic \
          -bios ./u-boot/spl/u-boot-spl \
          -device loader,file=./prototyper/target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.itb,addr=0x80200000 
```

## 使用U-Boot SPL和U-Boot启动Test Kerenl
### 编译RustSBI  Prototyper和Test Kernel

进入prototyper目录

``` shell
$ cd prototyper
```

编译RustSBI Prototyper和Test Kernel

``` shell
$ cargo make prototyper
$ cargo make test-kernel
```
本小节将使用二进制文件 `./target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper.bin`和`./target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin`。

### 编译U-Boot SPL

进入U-Boot目录

``` shell
$ cd u-boot
```

导出环境变量

``` shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
$ export OPENSBI=../prototyper/target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper.bin 
```

生成`.config`文件

``` shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
# add bootcmd value
$ make menuconfig
```

U-Boot 配置选项将加载到终端。导航到 `Boot options` $\rightarrow$ `bootcmd value` 并将以下内容写入 `bootcmd` 值：

``` 
ext4load virtio 0:1 84000000 rustsbi-test-kernel.bin; booti 0x84000000 - ${fdtcontroladdr}
```

编译U-Boot

``` shell
# To build U-Boot
$ make -j$(nproc)
```

本小节将使用二进制文件 `./spl/u-boot-spl`和`./u-boot.itb `。

### 创建启动盘
在`workshop`目录运行以下命令来创建一个256 MB的磁盘镜像

``` shell
# Create a 256 MB disk image
$ qemu-img create test-kernel.img 256m
```

#### 创建分区

将在磁盘映像`test-kernel.img`上创建1个分区，这个分区是可引导的。

`parted`命令将用于在镜像`test-kernel.img`中创建分区。在镜像中创建分区表：

``` shell
$ sudo parted test-kernel.img mklabel gpt
```

现在`test-kernel.img`中有一个分区表。将`test-kernel.img`挂载为loop device，以便它可以用作块设备。将`test-kernel.img`挂载为块设备将允许在其中创建分区。

``` shell
# Attach test-kernel.img with the first available loop device
$ sudo losetup --find --show test-kernel.img
```

> - `find`：查找第一个未使用的loop device
> - `show`：显示`test-kernel.img`附加到的loop device的名称

记下循环设备的完整路径。在本教程中它是`/dev/loop0`。对`/dev/loop0`的操作将会对`test-kernel.img`进行操作。

对`/dev/loop0`分区

``` shell
# Create a couple of primary partitions
$ sudo parted --align minimal /dev/loop0 mkpart primary ext4 0 100%

$ sudo parted /dev/loop0 print
```

#### 格式化分区

通过以下命令查看分区：

``` shell
$ ls -l /dev/loop0*
```

在本教程中，分区为`/dev/loop0p1`。

格式化分区并创建`ext4`文件系统，同时将分区设置为可引导分区。

``` shell
$ sudo mkfs.ext4 /dev/loop0p1

# Mark first partition as bootable
$ sudo parted /dev/loop0 set 1 boot on
```

#### 将Linux Kernel和根文件系统拷贝进启动盘

``` shell
# Mount the 1st partition
$ sudo mkdir test-kernel
$ sudo mount /dev/loop0p1 test-kernel
$ cd test-kernel
```
拷贝Linux Kernel镜像
``` shell
$ sudo cp ../prototyper/target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin .
```

卸载`test-kernel`

``` shell
$ cd workshop
$ sudo umount test-kernel
```

将`/dev/loop0`分离

``` shell
$ sudo losetup -d /dev/loop0
```

### 使用RustSBI 原型系统和U-Boot启动Linux Kernel

进入`workshop`目录

``` shell
$ cd workshop
```

运行下面命令

``` shell
$ qemu-system-riscv64 -M virt -smp 1 -m 256M -nographic \
          -bios ./u-boot/spl/u-boot-spl \
          -device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
          -blockdev driver=file,filename=./test-kernel.img,node-name=hd0 \
          -device virtio-blk-device,drive=hd0
```