# PolyOS 操作系统

PolyOS 是面向 RISC-V 智能终端和 AIoT 设备的开源操作系统，本节使用基于 OpenHarmony 的 PolyOS Mobile 镜像。

在 RISC-V 平台上，RustSBI 可为 PolyOS 提供 SBI 服务。本节以 QEMU `virt` 平台为例，介绍使用 RustSBI 和 U-Boot 启动 PolyOS 的方法。

## 使用 RustSBI 和 U-Boot 在 QEMU 中启动 PolyOS

本教程给出了使用 RustSBI 和 U-Boot 在 QEMU 中启动 PolyOS 的基本流程。

本教程使用的软件版本如下：

| 软件 | 版本 |
| :--: | :--: |
| qemu-system-riscv64 | 11.1.2 |
| Rust | 由 `rust-toolchain.toml` 指定 |
| cargo-binutils | 0.4.0 |
| RustSBI Prototyper | 0.0.0 |
| U-Boot | 2025.10 |
| PolyOS Mobile | 3.2-release |

### 准备 RustSBI Prototyper、U-Boot 和 PolyOS

本教程在 Linux 主机的 Bash 终端中操作，开始前请安装 Rustup。

在 Arch Linux 中安装所需软件：

```shell
$ sudo pacman -S git wget tar xz zstd base-devel e2fsprogs qemu-system-riscv qemu-ui-sdl qemu-hw-display-virtio-gpu qemu-hw-display-virtio-gpu-pci qemu-audio-sdl
```

在 Ubuntu 26.04 或更新版本中安装所需软件：

```shell
$ sudo apt install git wget tar xz-utils zstd build-essential binutils e2fsprogs qemu-system-riscv qemu-system-gui
```

在安同 OS 中安装所需软件：

```shell
$ sudo oma install git wget tar xz zstd gcc binutils e2fsprogs qemu
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

#### 下载 PolyOS 镜像文件

下载 [PolyOS Mobile 3.2-release 镜像](https://polyos.iscas.ac.cn/downloads/)及其校验文件，校验并解压：

```shell
$ wget https://polyos.iscas.ac.cn/downloads/polyos-mobile-3.2-release.img.tar.xz
$ wget https://polyos.iscas.ac.cn/downloads/polyos-mobile-3.2-release.img.tar.xz.sha256sum
$ sha256sum -c polyos-mobile-3.2-release.img.tar.xz.sha256sum
$ tar -xJf polyos-mobile-3.2-release.img.tar.xz
```

镜像包中的 `images/Image` 和 `images/ramdisk.img` 分别为内核和初始内存盘，其余 `.img` 文件为系统分区镜像。

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

### 制作启动镜像

创建存放内核、初始内存盘和引导配置的目录：

```shell
$ cd images
$ mkdir -p boot/extlinux
$ cp Image ramdisk.img boot
```

创建 `boot/extlinux/extlinux.conf`，并写入以下内容：

```shell
$ cat >boot/extlinux/extlinux.conf <<'EOF'
default polyOS-RISC-V
label polyOS-RISC-V
    kernel /Image
    initrd /ramdisk.img
    append loglevel=1 ip=192.168.137.2:192.168.137.1:192.168.137.1:255.255.255.0::eth0:off sn=0023456789 console=tty0,115200 console=ttyS0,115200 init=/bin/init ohos.boot.hardware=virt root=/dev/ram0 rw ohos.required_mount.system=/dev/block/vdb@/usr@ext4@ro,barrier=1@wait,required ohos.required_mount.vendor=/dev/block/vdc@/vendor@ext4@ro,barrier=1@wait,required ohos.required_mount.sys_prod=/dev/block/vde@/sys_prod@ext4@ro,barrier=1@wait,required ohos.required_mount.chip_prod=/dev/block/vdf@/chip_prod@ext4@ro,barrier=1@wait,required ohos.required_mount.data=/dev/block/vdd@/data@ext4@nosuid,nodev,noatime,barrier=1,data=ordered,noauto_da_alloc@wait,reservedsize=1073741824 ohos.required_mount.misc=/dev/block/vda@/misc@none@none=@wait,required
EOF
```

将 `boot` 目录中的文件写入 ext4 镜像，无需创建分区表或挂载镜像：

```shell
$ truncate -s 32M boot.img
$ mkfs.ext4 -F -d boot boot.img
```

### 使用 RustSBI Prototyper 和 U-Boot 启动 PolyOS

在 `workshop/images` 目录中运行下面命令。引导盘通过 AHCI 提供给 U-Boot，其余六个镜像保持下列 VirtIO 顺序，与内核参数中的 `/dev/block/vda` 至 `/dev/block/vdf` 对应：

```shell
$ qemu-system-riscv64 \
    -name PolyOS-Mobile \
    -machine virt,acpi=off \
    -m 4096 -smp 4 \
    -no-reboot \
    -bios ../rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
    -kernel ../u-boot.elf \
    -drive if=none,file=boot.img,format=raw,id=boot \
    -device ahci,id=ahci -device ide-hd,bus=ahci.0,drive=boot \
    -drive if=none,file=updater.img,format=raw,id=updater \
    -device virtio-blk-device,drive=updater \
    -drive if=none,file=system.img,format=raw,id=system \
    -device virtio-blk-device,drive=system \
    -drive if=none,file=vendor.img,format=raw,id=vendor \
    -device virtio-blk-device,drive=vendor \
    -drive if=none,file=userdata.img,format=raw,id=userdata \
    -device virtio-blk-device,drive=userdata \
    -drive if=none,file=sys_prod.img,format=raw,id=sys-prod \
    -device virtio-blk-device,drive=sys-prod \
    -drive if=none,file=chip_prod.img,format=raw,id=chip-prod \
    -device virtio-blk-device,drive=chip-prod \
    -serial mon:stdio \
    -device virtio-gpu-pci,xres=480,yres=864,max_outputs=1,addr=08.0 \
    -device virtio-mouse-pci \
    -device virtio-keyboard-pci \
    -device es1370 \
    -k en-us \
    -display sdl,gl=off
```

命令会打开图形窗口，串口输出显示在当前终端。在 U-Boot 的自动启动倒计时期间按任意键，进入命令行后执行：

```shell
=> scsi scan
=> setenv fdt_high 0x88000000
=> sysboot scsi 0:0 any ${scriptaddr} /extlinux/extlinux.conf
```

`fdt_high` 限制启动时设备树的复制位置，`scsi 0:0` 表示直接读取第一个 SCSI 磁盘上的文件系统。U-Boot 根据引导配置加载内核和初始内存盘，随后启动 PolyOS。

等待启动完成后，可在 QEMU 图形窗口中操作 PolyOS。
