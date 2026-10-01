# openEuler 操作系统

openEuler 是支持多种体系结构的开源操作系统项目，本节使用其 RISC-V 镜像。

在 RISC-V 平台上，RustSBI 可为 openEuler 提供 SBI 服务。本节以 QEMU `virt` 平台为例，介绍使用 RustSBI 和 U-Boot 启动 openEuler 的方法。

## 使用 RustSBI 和 U-Boot 在 QEMU 中启动 openEuler

本教程给出了使用 RustSBI 和 U-Boot 在 QEMU 中启动 openEuler 的基本流程。

本教程使用的软件版本如下：

| 软件 | 版本 |
| :--: | :--: |
| qemu-system-riscv64 | 11.1.2 |
| Rust | 由 `rust-toolchain.toml` 指定 |
| cargo-binutils | 0.4.0 |
| RustSBI Prototyper | 0.0.0 |
| U-Boot | 2025.10 |
| openEuler | 24.03 LTS SP4（RVA23） |

本教程使用的 RVA23 镜像要求 QEMU 10.0 或更高版本。

### 准备 RustSBI Prototyper、U-Boot 和 openEuler

本教程在 Linux 主机的 Bash 终端中操作，开始前请安装 Rustup。

在 Arch Linux 中安装所需软件：

```shell
$ sudo pacman -S git wget tar xz zstd base-devel qemu-system-riscv
```

在 Debian 13、Ubuntu 26.04 或更新版本中安装所需软件：

```shell
$ sudo apt install git wget tar xz-utils zstd build-essential binutils qemu-system-riscv
```

在安同 OS 中安装所需软件：

```shell
$ sudo oma install git wget tar xz zstd gcc binutils qemu
```

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

#### Clone RustSBI

```shell
$ git clone --depth 1 https://github.com/rustsbi/rustsbi.git
```

#### 下载预编译的 U-Boot

下载 Ubuntu 软件源提供的 QEMU RISC-V S 模式 U-Boot，提取预编译的 ELF 文件：

```shell
$ wget https://archive.ubuntu.com/ubuntu/pool/main/u/u-boot/u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb
$ ar p u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb data.tar.zst | tar --zstd -xOf - ./usr/lib/u-boot/qemu-riscv64_smode/uboot.elf > u-boot.elf
```

此 U-Boot 运行于内核态，由 RustSBI 提供 SBI 服务。

#### 下载 openEuler QEMU 磁盘镜像文件

下载并解压 [openEuler 24.03 LTS SP4 的 RISC-V 虚拟机镜像](https://repo.openeuler.org/openEuler-24.03-LTS-SP4/virtual_machine_img/riscv64/rva23/)：

```shell
$ wget https://repo.openeuler.org/openEuler-24.03-LTS-SP4/virtual_machine_img/riscv64/rva23/openEuler-24.03-LTS-SP4-riscv64-rva23.qcow2.xz
$ xz -d openEuler-24.03-LTS-SP4-riscv64-rva23.qcow2.xz
```

镜像已包含内核、根文件系统和 EFI 引导程序，无需另行编译内核或制作启动盘。

### 编译 RustSBI Prototyper

进入 RustSBI 目录

```shell
$ cd rustsbi
```

Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。

安装构建工具，编译 RustSBI Prototyper，完成后返回 `workshop` 目录

```shell
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
$ cargo prototyper build
$ cd ..
```

后续使用动态固件 `rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf`。

### 使用 RustSBI Prototyper 和 U-Boot 启动 openEuler

在 `workshop` 目录运行下面命令

```shell
$ qemu-system-riscv64 \
    -nographic -machine virt \
    -cpu rva23s64,sv39=on,pmp=on \
    -smp 4 -m 8G \
    -bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
    -kernel u-boot.elf \
    -drive file=openEuler-24.03-LTS-SP4-riscv64-rva23.qcow2,format=qcow2,if=none,id=hd0 \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-vga \
    -device virtio-rng-device,rng=rng0 \
    -device virtio-blk-device,drive=hd0 \
    -device virtio-net-device,netdev=usernet \
    -netdev user,id=usernet,hostfwd=tcp::12055-:22 \
    -device qemu-xhci -usb -device usb-kbd -device usb-tablet
```

`pmp=on` 启用 RustSBI 所需的物理内存保护（PMP）功能。U-Boot 自动扫描 VirtIO 磁盘，加载镜像中的 EFI 引导程序，再启动 openEuler。

系统启动后，以 `root` 账号登录，默认密码为 `openEuler12#$`。首次登录后，可使用 `passwd` 修改密码。

```shell
# passwd
```
