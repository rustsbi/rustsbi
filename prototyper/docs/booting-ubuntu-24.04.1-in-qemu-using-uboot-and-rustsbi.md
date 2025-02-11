# 使用RustSBI & U-Boot在QEMU中启动 Ubuntu 24.04.1

本教程给出了使用RustSBI和U-Boot在QEMU中启动Ubuntu 24.04.1的基本流程。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
| riscv64-linux-gnu-gcc | 14.1.0  |
|  qemu-system-riscv64  |  9.0.1  |
|  RustSBI Prototyper   |  0.0.0  |
|        U-Boot         | 2024.04 |

## 准备RustSBI Prototyper， U-Boot ，Ubuntu 24.04.1
创建工作目录并进入该目录

``` shell
$ mkdir workshop && cd workshop
```

### Clone RustSBI Prototyper

``` shell
$ git clone https://github.com/rustsbi/prototyper.git && cd prototyper && git checkout main && cd ..
```

### Clone U-Boot

``` shell
$ git clone https://github.com/u-boot/u-boot.git && cd u-boot && git checkout v2024.04 && cd ..
```
### 下载并扩容 Ubuntu 24.04.1 磁盘镜像文件

下载链接：[Ubuntu 24.04.1](https://cdimage.ubuntu.com/releases/noble/release/ubuntu-24.04.1-preinstalled-server-riscv64.img.xz)
```shell
 $ unar ubuntu-24.04.1-preinstalled-server-riscv64.img.xz
 $ qemu-img resize -f raw ubuntu-24.04.1-preinstalled-server-riscv64.img +5G
```

- The password of the default user `ubuntu` is `ubuntu`.
- 登录后应会被要求更改登录密码。
- 可以通过 `sudo` 更改 root 密码。


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

生成`.config`文件,编译U-Boot

``` shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
$ make -j$(nproc)
```

## 使用RustSBI 原型系统和U-Boot启动 Ubuntu 24.04.1

进入`workshop`目录

``` shell
$ cd workshop
```

运行下面命令

``` shell
$ qemu-system-riscv64 \
    -nographic -machine virt \
    -smp 4 -m 8G \
    -bios ./u-boot/spl/u-boot-spl  \
    -device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
    -drive file=ubuntu-24.04.1-preinstalled-server-riscv64.img,format=raw,if=none,id=hd0 \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-vga \
    -device virtio-rng-device,rng=rng0 \
    -device virtio-blk-device,drive=hd0 \
    -device virtio-net-device,netdev=usernet \
    -netdev user,id=usernet,hostfwd=tcp::12055-:22 \
    -device qemu-xhci -usb -device usb-kbd -device usb-tablet
```
