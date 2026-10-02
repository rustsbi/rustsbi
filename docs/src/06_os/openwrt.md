# OpenWrt 操作系统

[OpenWrt](https://openwrt.org/) 是面向路由器等嵌入式网络设备的开源 Linux 发行版，提供可定制的网络功能和软件包管理系统。

在 RISC-V 平台上，RustSBI 可为 OpenWrt 的 Linux 内核提供 SBI 服务。本节以 QEMU `virt` 平台为例，介绍使用 RustSBI 和 U-Boot 启动 OpenWrt 的方法。

## 使用 RustSBI 和 U-Boot 在 QEMU 中启动 OpenWrt

本教程给出了使用 RustSBI 和 U-Boot 在 QEMU 中启动 OpenWrt 的基本流程。

在已安装 RISC-V GNU 工具链和 QEMU 的情况下，完成本教程预计需要 10～30 分钟，实际耗时取决于主机性能和网络状况。

本教程使用软件版本如下：

|         软件         |        版本        |
| :------------------: | :----------------: |
| riscv64-linux-gnu-gcc |       16.2.0       |
|       Binutils       |        2.47        |
| qemu-system-riscv64   |       11.1.2       |
| Rust                 | 由 `rust-toolchain.toml` 指定 |
| RustSBI Prototyper    |       0.0.0        |
| U-Boot               |      2026.07       |
| OpenWrt              |      25.12.5       |

OpenWrt 25.12.5 尚未提供 RISC-V QEMU `virt` 专用镜像。本教程使用官方 SiFive Unleashed 镜像中的预编译内核和根文件系统，保留默认内核配置，并将根文件系统打包为 initramfs，在 QEMU `virt` 中运行。以这种方式启动时，在 OpenWrt 中修改的配置和文件只保存在虚拟机内存中，不会写回主机上的 initramfs 文件。重新启动 QEMU 后，这些修改会丢失。

### 准备 RustSBI Prototyper、U-Boot 和 OpenWrt

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

在 Ubuntu 中安装构建和镜像处理工具：

```shell
$ sudo apt install build-essential bison flex bc libssl-dev libgnutls28-dev python3-dev python3-setuptools python3-pyelftools swig curl cpio e2fsprogs mtools util-linux
```

#### Clone RustSBI

```shell
$ git clone -b main https://github.com/rustsbi/rustsbi.git
```

#### Clone U-Boot

```shell
$ git clone -b v2026.07 https://github.com/u-boot/u-boot.git
```

#### 下载 OpenWrt

```shell
$ mkdir openwrt
$ curl -fL https://downloads.openwrt.org/releases/25.12.5/targets/sifiveu/generic/openwrt-25.12.5-sifiveu-generic-sifive_unleashed-ext4-sdcard.img.gz -o openwrt/openwrt.img.gz
$ printf '%s\n' '41ab77db11d689d3fde501dfd6f9a64f1adc0a53b6b2fb66eb3dabc635e3e0c7  openwrt/openwrt.img.gz' | sha256sum -c -
```

### 编译 RustSBI Prototyper

进入 RustSBI 目录

```shell
$ cd rustsbi
```

Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。

安装构建工具，编译 RustSBI Prototyper

```shell
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
$ cargo prototyper build
```

### 编译 U-Boot SPL

进入 U-Boot 目录

```shell
$ cd ../u-boot
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
$ export OPENSBI=../rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin
```

生成 `.config` 文件，编译 U-Boot

```shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
$ make -j$(nproc)
```

### 准备 OpenWrt 启动镜像

进入 OpenWrt 目录，解压镜像并提取内核和根文件系统。第三分区存放内核，第四分区存放根文件系统；`partx` 输出的位置和长度以 512 字节扇区为单位：

```shell
$ cd ../openwrt
$ gzip -dc openwrt.img.gz > openwrt.img
$ boot_start=$(partx --raw --noheadings --nr 3 --output START openwrt.img)
$ read root_start root_size < <(partx --raw --noheadings --nr 4 --output START,SECTORS openwrt.img)
$ mcopy -i "openwrt.img@@$((boot_start * 512))" ::Image Image.gz
$ gzip -dc Image.gz > Image
$ dd if=openwrt.img of=rootfs.ext4 bs=512 skip="$root_start" count="$root_size" status=none
$ mkdir rootfs
$ debugfs -R 'rdump / rootfs' rootfs.ext4
```

补充 QEMU `virt` 的 `ttyS0` 登录配置和 `/init` 入口，然后打包为 initramfs：

```shell
$ printf '%s\n' 'ttyS0::askfirst:/usr/libexec/login.sh' >> rootfs/etc/inittab
$ ln -s sbin/init rootfs/init
$ (cd rootfs && find . -print0 | cpio --null -o --format=newc --owner=0:0 --quiet | gzip -9) > openwrt-initramfs.cpio.gz
```

以普通用户提取文件时，`debugfs` 可能提示无法保留文件所有者；上面的 `--owner=0:0` 会在打包时将文件所有者统一设为 root。

### 使用 RustSBI Prototyper 和 U-Boot 启动 OpenWrt

返回 `workshop` 目录，生成稍后需要在 U-Boot 中执行的启动命令：

```shell
$ cd ..
$ printf 'setenv bootargs console=ttyS0 earlycon=sbi; booti 0x88000000 0x8a000000:0x%x ${fdtcontroladdr}\n' "$(stat -c %s openwrt/openwrt-initramfs.cpio.gz)"
```

复制输出的启动命令，然后运行 QEMU。出现 `Hit any key to stop autoboot` 时按任意键，在 U-Boot 的 `=>` 提示符后粘贴并执行该命令：

```shell
$ qemu-system-riscv64 \
-machine virt -nographic -m 1G -smp 1 \
-bios ./u-boot/spl/u-boot-spl \
-device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
-device loader,file=./openwrt/Image,addr=0x88000000 \
-device loader,file=./openwrt/openwrt-initramfs.cpio.gz,addr=0x8a000000
```

出现 `Please press Enter to activate this console` 后按回车，即可进入 OpenWrt 命令行。
