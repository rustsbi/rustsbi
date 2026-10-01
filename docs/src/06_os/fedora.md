# Fedora 操作系统

Fedora 是由社区开发的开源 Linux 发行版。

在 RISC-V 平台上，RustSBI 可为 Fedora 提供 SBI 服务。本节以 QEMU `virt` 平台为例，介绍使用 RustSBI 和 U-Boot 启动 Fedora 的方法。

## 使用 RustSBI 和 U-Boot 在 QEMU 中启动 Fedora

本教程给出了使用 RustSBI 和 U-Boot 在 QEMU 中启动 Fedora 的基本流程。

本教程使用的软件版本如下：

| 软件 | 版本 |
| :--: | :--: |
| qemu-system-riscv64 | 11.1.2 |
| Rust | 由 `rust-toolchain.toml` 指定 |
| cargo-binutils | 0.4.0 |
| RustSBI Prototyper | 0.0.0 |
| U-Boot | 2025.10 |
| Fedora Cloud | 44（20260604.0） |

### 准备 RustSBI Prototyper、U-Boot 和 Fedora

本教程在 Linux 主机的 Bash 终端中操作，开始前请安装 Rustup。

在 Arch Linux 中安装所需软件：

```shell
$ sudo pacman -S git wget tar zstd base-devel qemu-system-riscv qemu-hw-display-virtio-vga qemu-hw-display-virtio-gpu seabios cloud-image-utils
```

在 Ubuntu 26.04 或更新版本中安装所需软件：

```shell
$ sudo apt install git wget tar zstd build-essential binutils qemu-system-riscv cloud-image-utils
```

在安同 OS 中安装所需软件：

```shell
$ sudo oma install git wget tar zstd gcc binutils qemu cloud-utils
```

软件源提供的 QEMU 版本可能与上表不同。

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

#### 下载 Fedora 镜像文件

下载 [Fedora 44 的 RISC-V Cloud 镜像](https://dl.fedoraproject.org/pub/alt/risc-v/release/44/Cloud/riscv64/images/)：

```shell
$ wget https://dl.fedoraproject.org/pub/alt/risc-v/release/44/Cloud/riscv64/images/Fedora-Cloud-Base-Generic-44-20260604.0.riscv64.qcow2
```

镜像已包含内核、根文件系统和 EFI 引导程序，无需另行编译内核或制作启动盘。

### 编译 RustSBI Prototyper

Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。安装构建工具，编译 RustSBI Prototyper，完成后返回 `workshop` 目录：

```shell
$ cd rustsbi
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
$ cargo prototyper build
$ cd ..
```

后续使用动态固件 `rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf`。

### 配置 cloud-init

Fedora Cloud 镜像默认账号为 `fedora`，没有默认密码。创建登录配置，将 `password` 项的值替换为需要使用的登录密码：

```shell
$ cat >user-data <<'EOF'
#cloud-config
password: password
chpasswd:
  expire: false
ssh_pwauth: true
EOF
$ cloud-localds seed.img user-data
```

`cloud-localds` 自动生成元数据并创建种子镜像，`ssh_pwauth` 允许通过 SSH 使用密码登录。

### 使用 RustSBI Prototyper 和 U-Boot 启动 Fedora

在 `workshop` 目录中运行下面命令。`acpi=off` 使系统使用设备树启动：

```shell
$ qemu-system-riscv64 \
    -nographic -machine virt,acpi=off \
    -smp 4 -m 8G \
    -bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
    -kernel u-boot.elf \
    -drive file=Fedora-Cloud-Base-Generic-44-20260604.0.riscv64.qcow2,format=qcow2,if=none,id=hd0 \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-vga \
    -device virtio-rng-device,rng=rng0 \
    -device virtio-blk-device,drive=hd0 \
    -device virtio-net-device,netdev=usernet \
    -netdev user,id=usernet,hostfwd=tcp::12055-:22 \
    -device qemu-xhci -usb -device usb-kbd -device usb-tablet \
    -drive file=seed.img,format=raw,if=none,id=seed,readonly=on \
    -device virtio-blk-device,drive=seed
```

U-Boot 自动扫描 VirtIO 磁盘，加载镜像中的 EFI 引导程序，再启动 Fedora。

等待 cloud-init 初始化完成后，以 `fedora` 账号和前面设置的密码登录。
