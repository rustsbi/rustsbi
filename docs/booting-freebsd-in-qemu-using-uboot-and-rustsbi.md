# 使用RustSBI & U-Boot在QEMU中启动FreeBSD

本教程给出了使用RustSBI和U-Boot在QEMU中启动FreeBSD的基本流程。

请读者在其主机上安装必要的软件来尝试本教程的脚本。本教程是在Arch Linux上开发的。

RustSBI 原型系统提供动态固件，根据前一个阶段传入的信息动态加载下一个阶段。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
| riscv64-linux-gnu-gcc | 14.2.0  |
|  qemu-system-riscv64  |  9.1.1  |
|  RustSBI Prototyper   |  0.0.0  |
|        U-Boot         | 2024.04 |
|       FreeBSD         |  14.1   |

## 环境配置

### 安装交叉编译器和QEMU

For Arch Linux

``` shell
$ sudo pacman -S git riscv64-linux-gnu-gcc qemu-system-riscv
```

#### 测试是否成功安装

For riscv64-linux-gnu-gcc:

``` shell
$ riscv64-linux-gnu-gcc --version
```

它将输出以下版本信息

```
riscv64-linux-gnu-gcc (GCC) 14.2.0
Copyright (C) 2024 Free Software Foundation, Inc.
This is free software; see the source for copying conditions.  There is NO
warranty; not even for MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
```

For QEMU:

``` shell
$ qemu-system-riscv64 --version
```

它将输出以下版本信息

```
QEMU emulator version 9.1.1
Copyright (c) 2003-2024 Fabrice Bellard and the QEMU Project developers
```

### 准备RustSBI Prototyper， U-Boot和FreeBSD镜像

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

Download FreeBSD
``` shell
$ wget https://download.freebsd.org/releases/VM-IMAGES/14.1-RELEASE/riscv64/Latest/FreeBSD-14.1-RELEASE-riscv-riscv64.raw.xz && xz -d FreeBSD-14.1-RELEASE-riscv-riscv64.raw.xz
```


## 编译RustSBI  Prototyper

进入prototyper目录

``` shell
$ cd prototyper
```

编译RustSBI  Prototyper

``` shell
$ cargo prototyper
```

## 编译U-Boot SPL

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
```

编译U-Boot

``` shell
# To build U-Boot
$ make -j$(nproc)
```

本小节将使用二进制文件 `./spl/u-boot-spl`和`./u-boot.itb `。

## 使用RustSBI 原型系统和U-Boot启动Linux Kernel

进入`workshop`目录

``` shell
$ cd workshop
```

运行下面命令

``` shell
$ qemu-system-riscv64 -M virt -smp 1 -m 256M -nographic \
          -bios ./u-boot/spl/u-boot-spl \
          -device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
          -blockdev driver=file,filename=./FreeBSD-14.1-RELEASE-riscv-riscv64.raw,node-name=hd0 \
          -device virtio-blk-device,drive=hd0
```