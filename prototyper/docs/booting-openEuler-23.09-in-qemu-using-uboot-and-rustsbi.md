# 使用RustSBI & U-Boot在QEMU中启动openEuler 23.09

本教程给出了使用RustSBI和U-Boot在QEMU中启动openEuler 23.09的基本流程。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
| riscv64-linux-gnu-gcc | 14.1.0  |
|  qemu-system-riscv64  |  9.0.1  |
|  RustSBI Prototyper   |  0.0.0  |
|        U-Boot         | 2024.04 |

## 准备RustSBI Prototyper， U-Boot ，openEuler 23.09
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
### 下载openEuler 23.09 Qemu磁盘镜像文件

下载链接：[openEuler 23.09](https://mirror.iscas.ac.cn/openeuler-sig-riscv/openEuler-RISC-V/preview/openEuler-23.09-V1-riscv64/QEMU/openEuler-23.09-V1-base-qemu-preview.qcow2.zst)
```shell
 $ unzstd openEuler-23.09-V1-base-qemu-preview.qcow2.zst
```
- The password of user `root` is `openEuler12#$`.
- The password of the default user `openeuler` is `openEuler12#$`.


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
$ ./scripts/config -e CMD_BTRFS -e FS_BTRFS
$ make olddefconfig
$ sed -i.bak 's/# CONFIG_USE_BOOTARGS is not set/CONFIG_USE_BOOTARGS=y\nCONFIG_BOOTARGS="root=\/dev\/vda1 rw console=ttyS0 swiotlb=1 loglevel=7 systemd.default_timeout_start_sec=600 selinux=0 highres=off earlycon"/' .config
$ make -j$(nproc)
```

## 使用RustSBI 原型系统和U-Boot启动openEuler 23.09

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
    -drive file=openEuler-23.09-V1-base-qemu-preview.qcow2,format=qcow2,id=hd0 \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-vga \
    -device virtio-rng-device,rng=rng0 \
    -device virtio-blk-device,drive=hd0 \
    -device virtio-net-device,netdev=usernet \
    -netdev user,id=usernet,hostfwd=tcp::12055-:22 \
    -device qemu-xhci -usb -device usb-kbd -device usb-tablet
```
