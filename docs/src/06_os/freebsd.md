# FreeBSD 操作系统

FreeBSD 是开源的类 Unix 操作系统，提供内核、用户态工具和系统文档。

在 RISC-V 平台上，RustSBI 可为 FreeBSD 提供 SBI 服务。本节以 QEMU `virt` 平台为例，介绍使用 RustSBI 和 U-Boot 启动 FreeBSD 的方法。

## 使用 RustSBI 和 U-Boot 在 QEMU 中启动 FreeBSD

本教程给出了使用 RustSBI 和 U-Boot 在 QEMU 中启动 FreeBSD 的基本流程。

以下命令在 Linux 主机的 Bash 终端中执行。开始前，请安装 Rustup。

RustSBI Prototyper 提供动态固件，根据上一引导阶段传入的信息确定下一阶段的入口并跳转。

本教程使用的软件版本如下：

| 软件 | 版本 |
| :--: | :--: |
| qemu-system-riscv64 | 11.1.2 |
| Rust | 由 `rust-toolchain.toml` 指定 |
| cargo-binutils | 0.4.0 |
| RustSBI Prototyper | 0.0.0 |
| U-Boot | 2025.10 |
| FreeBSD | 15.1-RELEASE |

### 环境配置

#### 安装 QEMU 和构建工具

在 Arch Linux 中安装所需软件：

```shell
$ sudo pacman -S git wget tar xz zstd base-devel qemu-system-riscv
```

在 Debian 或 Ubuntu 中安装所需软件：

```shell
$ sudo apt install git wget tar xz-utils zstd build-essential binutils qemu-system-misc
```

在安同 OS 中安装所需软件：

```shell
$ sudo oma install git wget tar xz zstd gcc binutils qemu
```

软件源提供的 QEMU 版本可能与上表不同。

#### 准备 RustSBI Prototyper、U-Boot 和 FreeBSD 镜像

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

Clone RustSBI

```shell
$ git clone --depth 1 https://github.com/rustsbi/rustsbi.git
```

下载 Ubuntu 软件源提供的 QEMU RISC-V S 模式 U-Boot，提取预编译的 ELF 文件：

```shell
$ wget https://archive.ubuntu.com/ubuntu/pool/main/u/u-boot/u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb
$ ar p u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb data.tar.zst | tar --zstd -xOf - ./usr/lib/u-boot/qemu-riscv64_smode/uboot.elf > u-boot.elf
```

此 U-Boot 运行于内核态，由 RustSBI 提供 SBI 服务。

下载并解压 [FreeBSD 15.1-RELEASE 的 RISC-V UFS 虚拟机镜像](https://download.freebsd.org/releases/VM-IMAGES/15.1-RELEASE/riscv64/Latest/)：

```shell
$ wget https://download.freebsd.org/releases/VM-IMAGES/15.1-RELEASE/riscv64/Latest/FreeBSD-15.1-RELEASE-riscv-riscv64-ufs.raw.xz
$ xz -d FreeBSD-15.1-RELEASE-riscv-riscv64-ufs.raw.xz
```

镜像已包含内核、根文件系统和 EFI 引导程序，无需另行编译 FreeBSD 内核或制作启动盘。

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

### 使用 RustSBI Prototyper 和 U-Boot 启动 FreeBSD

在 `workshop` 目录运行下面命令

```shell
$ qemu-system-riscv64 \
-machine virt -smp 1 -m 1G -nographic \
-bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
-kernel u-boot.elf \
-drive file=FreeBSD-15.1-RELEASE-riscv-riscv64-ufs.raw,format=raw,if=none,id=hd0 \
-object rng-random,filename=/dev/urandom,id=rng0 \
-device virtio-vga \
-device virtio-rng-device,rng=rng0 \
-device virtio-blk-device,drive=hd0 \
-device virtio-net-device,netdev=usernet \
-netdev user,id=usernet
```

U-Boot 自动扫描 VirtIO 磁盘，加载镜像中的 EFI 引导程序，再启动 FreeBSD。

系统启动后，以 `root` 账号登录，默认无需密码。首次登录后，可使用 `passwd` 设置密码。

```shell
# passwd
```

检查系统版本和处理器架构：

```shell
# freebsd-version
# uname -m
# sysctl hw.machine_arch
```

预期分别输出 `15.1-RELEASE`、`riscv` 和 `hw.machine_arch: riscv64`。
