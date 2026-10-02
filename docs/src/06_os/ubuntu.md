# Ubuntu 操作系统

[Ubuntu](https://ubuntu.com/) 是基于 Linux 的操作系统，提供适用于 RISC-V 平台的服务器预安装镜像。

本节以 QEMU `virt` 平台为例，介绍使用 RustSBI 和 U-Boot 或 EDK2 启动 Ubuntu 的方法。两种方式使用同一版本的镜像，可按需选择。

本教程使用的软件版本如下：

| 软件 | 版本 |
| :--: | :--: |
| qemu-system-riscv64 | 11.1.2 |
| Rust | 由 `rust-toolchain.toml` 指定 |
| cargo-binutils | 0.4.0 |
| RustSBI Prototyper | 0.0.0 |
| U-Boot | 2025.10 |
| EDK2 | 2025.11 |
| Ubuntu | 26.04.1 LTS |

Ubuntu 26.04.1 的 RISC-V 镜像要求 RVA23S64 指令集配置和 QEMU 10.1 或更高版本。

## 准备 RustSBI 和 Ubuntu 镜像

以下命令在 Linux 主机的 Bash 终端中执行。开始前，请安装 Rustup。

### 安装 QEMU 和构建工具

在 Arch Linux 中安装所需软件：

```shell
$ sudo pacman -S git wget tar xz zstd base-devel qemu-system-riscv qemu-hw-display-virtio-vga qemu-hw-display-virtio-gpu seabios
```

在 Ubuntu 26.04 或更新版本中安装所需软件：

```shell
$ sudo apt install git wget tar xz-utils zstd build-essential binutils qemu-system-riscv
```

在安同 OS 中安装所需软件：

```shell
$ sudo oma install git wget tar xz zstd gcc binutils qemu
```

软件源提供的 QEMU 版本可能与上表不同，请使用满足上述要求的版本。

### Clone RustSBI

创建工作目录并进入该目录，下载 RustSBI 源码：

```shell
$ mkdir workshop && cd workshop
$ git clone --depth 1 https://github.com/rustsbi/rustsbi.git
```

### 下载 Ubuntu 磁盘镜像

下载并解压 [Ubuntu 26.04.1 LTS 的 RISC-V 服务器预安装镜像](https://cdimage.ubuntu.com/releases/26.04.1/release/)：

```shell
$ wget https://cdimage.ubuntu.com/releases/26.04.1/release/ubuntu-26.04.1-preinstalled-server-riscv64.img.xz
$ xz -d ubuntu-26.04.1-preinstalled-server-riscv64.img.xz
```

镜像已包含内核、根文件系统和 EFI 引导程序，无需另行编译内核或制作启动盘。

### 编译 RustSBI Prototyper

Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。安装构建工具，并编译 RustSBI Prototyper：

```shell
$ cd rustsbi
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
$ cargo prototyper build
$ cd ..
```

后续使用动态固件 `rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf`。

## 使用 RustSBI 和 U-Boot 在 QEMU 中启动 Ubuntu

### 下载预编译的 U-Boot

在 `workshop` 目录下载 Ubuntu 软件源提供的 QEMU RISC-V S 模式 U-Boot，提取预编译的 ELF 文件：

```shell
$ wget https://archive.ubuntu.com/ubuntu/pool/main/u/u-boot/u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb
$ ar p u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb data.tar.zst | tar --zstd -xOf - ./usr/lib/u-boot/qemu-riscv64_smode/uboot.elf > u-boot.elf
```

此 U-Boot 运行于内核态，由 RustSBI 提供 SBI 服务。

### 启动 Ubuntu

在 `workshop` 目录运行下面命令。`pmp=on` 启用 RustSBI 所需的物理内存保护（PMP）功能，`acpi=off` 使系统使用设备树启动：

```shell
$ qemu-system-riscv64 \
    -nographic -machine virt,acpi=off \
    -cpu rva23s64,sv39=on,pmp=on \
    -smp 4 -m 8G \
    -bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
    -kernel u-boot.elf \
    -drive file=ubuntu-26.04.1-preinstalled-server-riscv64.img,format=raw,if=none,id=hd0 \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-vga \
    -device virtio-rng-device,rng=rng0 \
    -device virtio-blk-device,drive=hd0 \
    -device virtio-net-device,netdev=usernet \
    -netdev user,id=usernet,hostfwd=tcp::12055-:22 \
    -device qemu-xhci -usb -device usb-kbd -device usb-tablet
```

U-Boot 自动扫描 VirtIO 磁盘，加载镜像中的 EFI 引导程序，再启动 Ubuntu。

## 使用 RustSBI 和 EDK2 在 QEMU 中启动 Ubuntu

### 下载预编译的 EDK2

在 `workshop` 目录下载 Ubuntu 软件源提供的 RISC-V EDK2 固件，提取代码和变量存储文件：

```shell
$ wget https://archive.ubuntu.com/ubuntu/pool/main/e/edk2/qemu-efi-riscv64_2025.11-3ubuntu7.2_all.deb
$ ar p qemu-efi-riscv64_2025.11-3ubuntu7.2_all.deb data.tar.zst | tar --zstd -xf - --strip-components=4 ./usr/share/qemu-efi-riscv64/RISCV_VIRT_{CODE,VARS}.fd
```

两份固件文件均为 32 MiB，无需扩容。`RISCV_VIRT_CODE.fd` 存放固件代码，`RISCV_VIRT_VARS.fd` 存放 UEFI 变量，后者需要可写。

### 启动 Ubuntu

在 `workshop` 目录运行下面命令：

```shell
$ qemu-system-riscv64 \
    -nographic -machine virt,acpi=off \
    -cpu rva23s64,sv39=on,pmp=on \
    -smp 4 -m 8G \
    -bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
    -drive if=pflash,format=raw,unit=0,file=RISCV_VIRT_CODE.fd,readonly=on \
    -drive if=pflash,format=raw,unit=1,file=RISCV_VIRT_VARS.fd \
    -drive file=ubuntu-26.04.1-preinstalled-server-riscv64.img,format=raw,if=none,id=hd0 \
    -device virtio-blk-device,drive=hd0 \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-rng-device,rng=rng0 \
    -netdev user,id=usernet,hostfwd=tcp::12055-:22 \
    -device virtio-net-device,netdev=usernet
```

EDK2 自动加载镜像中的 EFI 引导程序，再启动 Ubuntu。

## 登录 Ubuntu

等待串口终端中出现 cloud-init 初始化完成的信息后，以 `ubuntu` 账号登录，默认密码为 `ubuntu`。首次登录时，系统会要求更改密码。

登录后，可通过以下命令检查 UEFI 固件接口；预期输出为 `64`：

```shell
$ cat /sys/firmware/efi/fw_platform_size
```

```text
64
```
