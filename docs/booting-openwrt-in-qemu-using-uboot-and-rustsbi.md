# 使用RustSBI & U-Boot在QEMU中启动 Openwrt

本教程给出了使用RustSBI和U-Boot在QEMU中启动Openwrt的基本流程。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
| riscv64-linux-gnu-gcc | 14.1.0  |
|  qemu-system-riscv64  |  9.0.1  |
|  RustSBI Prototyper   |  0.0.0  |
|        U-Boot         | 2024.04 |

## 准备RustSBI Prototyper， U-Boot ，Openwrt
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

### Clone & Patch Openwrt

``` shell
$ git clone https://git.openwrt.org/openwrt/openwrt.git 
$ cd ./openwrt
$ git checkout 603a3c6
```

应用本项目目录下的 `docs/openwrt-patch.patch`。

```shell
$ curl https://raw.githubusercontent.com/rustsbi/prototyper/refs/heads/main/docs/openwrt-patch.patch --output openwrt-patch.patch
$ git apply openwrt-patch.patch
```

## 编译RustSBI  Prototyper

进入prototyper目录

``` shell
$ cd prototyper
```

编译RustSBI  Prototyper

``` shell
$ cargo make prototyper
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
$ sed -i.bak 's/CONFIG_BOOTCOMMAND=*/CONFIG_BOOTCOMMAND="scsi scan; fatload scsi 0:3 84000000 Image; setenv bootargs root=\/dev\/sda4 rw earlycon console=\/dev\/ttyS0 rootwait; booti 0x84000000 - ${fdtcontroladdr};"/' .config
$ make -j$(nproc)
```

## 编译 Openwrt

首先，你应先按照 <https://openwrt.org/docs/guide-developer/toolchain/install-buildsystem> 配置自己的编译环境。

（以下内容参照并修改自 <https://openwrt.org/docs/guide-developer/toolchain/use-buildsystem>）

更新 Feeds：
```shell
$ cd openwrt
# Update the feeds
$ ./scripts/feeds update -a
$ ./scripts/feeds install -a
```

修改配置：
```shell
$ make -j$(nproc) menuconfig
```

进入 `Target System`，选中 `$SiFive U-based RISC-V boards`。

修改内核配置：
```shell
$ make -j$(nproc) kernel_menuconfig
```

进入后将   
`Device Drivers` $\rightarrow$ `Serial ATA and Parallel ATA drivers (libata)` $\rightarrow$ `AHCI SATA support`  
`Device Drivers` $\rightarrow$ `Network device support` $\rightarrow$ `Ethernet driver support` $\rightarrow$ `Intel devices` $\rightarrow$ `Intel(R) PRO/1000 Gigabit Ethernet support`  
设为 `built-in`。

编译镜像：
```shell
# Build the firmware image
$ make -j$(nproc) defconfig download clean world
```

拷贝并解压镜像：
```shell
$ cd ..
$ cp ./openwrt/bin/targets/sifiveu/generic/openwrt-sifiveu-generic-sifive_unleashed-ext4-sdcard.img.gz ./
$ gzip -dk openwrt-sifiveu-generic-sifive_unleashed-ext4-sdcard.img.gz
```

## 使用RustSBI 原型系统和U-Boot启动 Openwrt

进入`workshop`目录

``` shell
$ cd workshop
```

运行下面命令

``` shell
$ qemu-system-riscv64 \
-machine virt -nographic -m 4096 -smp 1 \
-bios ./u-boot/spl/u-boot-spl \
-device virtio-rng-pci -device ahci,id=ahci -device ide-hd,bus=ahci.0,drive=mydrive \
-drive file=./openwrt-sifiveu-generic-sifive_unleashed-ext4-sdcard.img,format=raw,if=none,id=mydrive \
-device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
-device e1000,netdev=n1 -netdev user,id=n1,hostfwd=tcp::12055-:22
```
