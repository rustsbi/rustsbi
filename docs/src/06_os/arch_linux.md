# Arch Linux 操作系统

Arch Linux 是使用 pacman 管理软件包的 Linux 发行版。其 RISC-V 移植提供了适用于 RISC-V 平台的软件包和根文件系统。

本节以 QEMU `virt` 平台为例，介绍使用 RustSBI 和 U-Boot，以及使用 RustSBI 和 EDK2 启动 Arch Linux 的方法。

本教程使用软件版本如下：

| 软件 | 版本 |
| :--: | :--: |
| riscv64-linux-gnu-gcc | 16.2.0 |
| Binutils | 2.47 |
| qemu-system-riscv64 | 11.1.2 |
| Rust | 由 `rust-toolchain.toml` 指定 |
| cargo-binutils | 0.4.0 |
| RustSBI Prototyper | 0.0.0 |
| Linux | 7.2.8 |
| U-Boot | 2026.07 |
| EDK2 | edk2-stable202608 |
| Arch Linux RISC-V 根文件系统 | 2026-08-27 |

## 准备工作

以下命令在 Linux 主机的 Bash 终端中执行。两种启动方式共用 RustSBI 动态固件、Linux 内核和根文件系统镜像，可以按需选择 U-Boot 或 EDK2。

开始前，请安装 Rustup。

### 安装构建工具

在 Arch Linux 中安装交叉编译器、QEMU、构建工具和镜像处理工具：

```shell
$ sudo pacman -S git wget base-devel bc openssl zstd riscv64-linux-gnu-gcc qemu-system-riscv dosfstools e2fsprogs mtools
```

在 Ubuntu 中安装所需软件：

```shell
$ sudo apt install git wget build-essential bc bison flex openssl libssl-dev zstd xz-utils gcc-riscv64-linux-gnu qemu-system-misc dosfstools e2fsprogs mtools
```

软件源提供的版本可能与上表不同，请准备上表所列版本的 RISC-V GNU 工具链和 QEMU。

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

### 编译 RustSBI Prototyper

下载 RustSBI 源码并进入该目录。Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。

```shell
$ git clone --depth 1 https://github.com/rustsbi/rustsbi.git
$ cd rustsbi
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
$ cargo prototyper build
$ cd ..
```

后续使用动态固件 `rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf`。

### 创建根文件系统镜像

下载 [Arch Linux RISC-V 根文件系统](https://archriscv.felixc.at/images/)，解压到 `rootfs` 目录：

```shell
$ wget https://archriscv.felixc.at/images/archriscv-2026-08-27.tar.zst
$ mkdir rootfs
$ sudo tar --zstd --numeric-owner -xf archriscv-2026-08-27.tar.zst -C rootfs
```

设置 root 账号的密码，按提示输入并确认密码：

```shell
$ sudo usermod --prefix "$(realpath rootfs)" --password "$(openssl passwd -6)" root
```

创建 4G 的 EXT4 根文件系统镜像，大小可以按需调整：

```shell
$ truncate -s 4G archriscv.img
$ sudo mkfs.ext4 -F -d rootfs archriscv.img
```

`-d rootfs` 将目录中的内容写入镜像，无需挂载镜像或创建分区。

### 编译 Linux RISC-V 内核

下载并编译 Linux 7.2.8：

```shell
$ wget https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-7.2.8.tar.xz
$ tar xf linux-7.2.8.tar.xz
$ export ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu-
$ make -C linux-7.2.8 defconfig
$ make -C linux-7.2.8 -j$(nproc) Image
$ cp linux-7.2.8/arch/riscv/boot/Image .
```

默认配置将 VirtIO 块设备、EXT4 和 EFI STUB 等启动所需功能编译进内核，本教程无需安装内核模块或生成 initramfs。同一份 `Image` 可用于 U-Boot 和 EDK2 启动。

## 使用 RustSBI 和 U-Boot 在 QEMU 中启动 Arch Linux

### 下载并编译 U-Boot

以下依赖仅用于编译 U-Boot。若选择 EDK2 启动方式，可跳过本小节。

在 Arch Linux 中安装：

```shell
$ sudo pacman -S gnutls python python-setuptools python-pyelftools swig
```

在 Ubuntu 中安装：

```shell
$ sudo apt install libgnutls28-dev python3-dev python3-setuptools python3-pyelftools swig
```

下载并编译 U-Boot。

```shell
$ git clone --depth 1 -b v2026.07 https://github.com/u-boot/u-boot.git
$ make -C u-boot qemu-riscv64_smode_defconfig
$ make -C u-boot -j$(nproc)
```

`qemu-riscv64_smode_defconfig` 用于构建运行于内核态的 U-Boot，由 RustSBI 提供 SBI 服务。

### 启动 Arch Linux

在 `workshop` 目录运行以下命令。出现 `Hit any key to stop autoboot` 时按任意键，中断自动启动：

```shell
$ qemu-system-riscv64 \
-machine virt -m 4G -smp 4 -nographic \
-bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
-kernel u-boot/u-boot.bin \
-device loader,file=Image,addr=0x84000000 \
-drive file=archriscv.img,format=raw,if=none,id=rootfs \
-device virtio-blk-device,drive=rootfs
```

在 U-Boot 的 `=>` 提示符后执行以下命令：

```shell
=> setenv bootargs 'root=/dev/vda rw rootwait console=ttyS0,115200'
=> booti 0x84000000 - ${fdtcontroladdr}
```

系统启动后，以 `root` 账号和前面设置的密码登录。

## 使用 RustSBI 和 EDK2 在 QEMU 中启动 Arch Linux

### 下载并编译 EDK2

以下依赖仅用于编译 EDK2。

在 Arch Linux 中安装：

```shell
$ sudo pacman -S nasm acpica python
```

在 Ubuntu 中安装：

```shell
$ sudo apt install nasm acpica-tools uuid-dev python3
```

下载 EDK2 源码，初始化子模块并编译固件：

```shell
$ git clone --depth 1 -b edk2-stable202608 https://github.com/tianocore/edk2.git
$ cd edk2
$ git submodule update --init --depth 1
$ export GCC_RISCV64_PREFIX=riscv64-linux-gnu-
$ source edksetup.sh
$ make -C BaseTools -j$(nproc)
$ build -a RISCV64 -b RELEASE -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC -n $(nproc)
$ cp Build/RiscVVirtQemu/RELEASE_GCC/FV/RISCV_VIRT_{CODE,VARS}.fd ..
$ cd ..
```

将 UEFI 固件文件扩展到 32 MiB，以符合 QEMU 对 pflash 固件的大小要求：

```shell
$ truncate -s 32M RISCV_VIRT_{CODE,VARS}.fd
```

### 创建 UEFI 启动镜像

创建 128 MiB 的 FAT32 启动镜像，将内核和 UEFI Shell 启动脚本写入镜像：

```shell
$ mkfs.fat -F 32 -C boot.img 131072
$ echo 'Image rw root=/dev/vda rootwait console=ttyS0,115200' > startup.nsh
$ mcopy -i boot.img Image startup.nsh ::
```

Linux 内核通过 [EFI STUB](https://docs.kernel.org/admin-guide/efi-stub.html) 作为 EFI 应用程序启动，`startup.nsh` 指定根文件系统所在的磁盘和串口终端。

### 启动 Arch Linux

在 `workshop` 目录运行以下命令。`acpi=off` 使系统使用设备树启动，根文件系统磁盘先于启动磁盘添加，对应 `/dev/vda`。

```shell
$ qemu-system-riscv64 \
-machine virt,acpi=off -m 4G -smp 4 -nographic \
-bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
-drive if=pflash,format=raw,unit=0,file=RISCV_VIRT_CODE.fd,readonly=on \
-drive if=pflash,format=raw,unit=1,file=RISCV_VIRT_VARS.fd \
-drive file=archriscv.img,format=raw,if=none,id=rootfs \
-device virtio-blk-device,drive=rootfs \
-drive file=boot.img,format=raw,if=none,id=boot \
-device virtio-blk-device,drive=boot
```

系统启动后，以 `root` 账号和前面设置的密码登录，检查 UEFI 固件接口：

```shell
$ cat /sys/firmware/efi/fw_platform_size
```

```text
64
```

输出 `64` 表示系统通过 64 位 UEFI 固件启动。
