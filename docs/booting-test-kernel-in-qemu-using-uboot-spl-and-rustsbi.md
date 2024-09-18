# 使用RustSBI & U-Boot SPL在QEMU中启动Test Kernel

本教程给出了使用RustSBI和U-Boot SPL在QEMU中启动Test Kernel的基本流程。

请读者在其主机上安装必要的软件来尝试本教程的脚本。本教程是在Arch Linux上开发的。

[环境配置](#环境配置)小节给出了本教程的环境配置方法，用户在使用本教程时需要先完成环境配置小节内容。


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

For Arhc Linux:

``` shell
$ sudo pacman -S git riscv64-linux-gnu-gcc qemu-system-riscv
```

For Ubuntu:

``` shell
$ sudo apt-get update && sudo apt-get upgrade
$ sudo apt-get install git qemu-system-misc gcc-riscv64-linux-gnu 
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

## 编译RustSBI  Prototyper

进入prototyper目录

``` shell
$ cd prototyper
```

编译RustSBI Prototyper和Test Kernel

``` shell
$ cargo make test-kernel-itb
```

本小节将使用二进制文件 `./target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.itb`。

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
# add bootcmd value
$ make menuconfig
```

编译U-Boot

``` shell
# To build U-Boot
$ make -j$(nproc)
```

本小节将使用二进制文件 `./spl/u-boot-spl`。

## 使用RustSBI 原型系统和U-Boot启动Linux Kernel

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

