# Asterinas 内核（不使用发行版）

[Asterinas](https://asterinas.github.io/book/) 是用 Rust 语言开发的通用操作系统内核，提供与 Linux ABI 兼容的用户态接口。

在 RISC-V 平台上，RustSBI 可为 Asterinas 提供 SBI 服务。本节以 QEMU `virt` 平台为例，介绍构建 Asterinas 内核及配套 initramfs，并使用 RustSBI 和 U-Boot 启动的方法。

## 在 QEMU 中启动 Asterinas

本教程给出了在 QEMU 中启动 Asterinas 的基本流程，分为两种类型：

1. 直接使用 RustSBI 启动 Asterinas。
2. 使用 RustSBI 和 U-Boot 启动 Asterinas。

本教程使用以下版本的软件：

|         软件         |             版本             |
| :------------------: | :--------------------------: |
| riscv64-linux-gnu-gcc |            16.2.0            |
| Binutils             |             2.47             |
| RustSBI Prototyper    |            0.0.0             |
| Asterinas            |            0.18.1            |
| QEMU                 |            11.1.2            |
| U-Boot               |           2026.07            |
| Nix（开发环境）       |            2.35.2            |

本教程在主机上构建 Asterinas 内核及配套的 initramfs，并通过 `AUTO_TEST=boot` 运行用户态启动测试。

### 准备 RustSBI Prototyper、Asterinas 和 U-Boot

本教程在 Ubuntu 24.04 或更新版本的 Bash 终端中操作。开始前，请安装 Rustup 和 QEMU 11.1.2。使用 U-Boot 启动时，还需安装 RISC-V GNU 工具链（GCC 16.2.0、Binutils 2.47）。

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

在 Ubuntu 中安装构建和文件系统工具，两种启动方式均需执行：

```shell
$ sudo apt install git curl build-essential python3 e2fsprogs exfatprogs
```

在编译主机上安装 [Nix](https://nix.dev/manual/nix/stable/installation/installing-binary.html)，用于交叉编译 initramfs 中的 RISC-V 用户态程序，并打包文件系统：

```shell
$ bash <(curl -L https://releases.nixos.org/nix/nix-2.35.2/install) --daemon
$ . /etc/profile.d/nix.sh
```

#### Clone RustSBI

```shell
$ git clone -b main https://github.com/rustsbi/rustsbi.git
```

#### Clone Asterinas

```shell
$ git clone --depth 1 -b v0.18.1 https://github.com/asterinas/asterinas.git
```

下载 Asterinas 所需的预编译 vDSO 库：

```shell
$ git clone https://github.com/asterinas/linux_vdso.git
$ git -C linux_vdso checkout 7489835
```

#### Clone U-Boot

仅在使用 RustSBI 和 U-Boot 启动 Asterinas 时需要安装以下依赖并下载 U-Boot。若直接使用 RustSBI 启动，可跳过本小节及后续使用 U-Boot 启动的小节。

```shell
$ sudo apt install bison flex bc gawk libssl-dev libgnutls28-dev python3-dev python3-setuptools python3-pyelftools swig dosfstools mtools
$ git clone -b v2026.07 https://github.com/u-boot/u-boot.git
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

### 编译 Asterinas

进入 Asterinas 目录，设置 vDSO 库的路径：

```shell
$ cd ../asterinas
$ export VDSO_LIBRARY_DIR="$(realpath ../linux_vdso)"
```

Asterinas 的 Rust 工具链及组件由其 `rust-toolchain.toml` 指定。构建过程会自动安装 OSDK，并通过 Nix 生成 initramfs。

```shell
$ rustup show
$ make kernel TARGET_ARCH=riscv64 RELEASE=1 AUTO_TEST=boot CONSOLE=ttyS0
```

创建存放启动文件的目录，复制内核和 initramfs，并写入启动参数：

```shell
$ mkdir boot
$ cp target/osdk/asterinas/asterinas-osdk-bin.qemu_elf boot/aster-nix-boot.elf
$ cp test/initramfs/build/initramfs.cpio.gz boot/initramfs-boot.cpio.gz
$ echo 'SHELL=/bin/sh LOGNAME=root HOME=/ USER=root PATH=/bin:/benchmark loglevel=error earlycon console=ttyS0 -- sh -l /test/boot_hello.sh' > boot/cmdline.txt
```

`console=ttyS0` 将内核终端设置为 QEMU `virt` 的串口。`--` 后的参数传递给 `/init`，用于执行用户态启动测试。后续两种启动流程使用相同的内核、initramfs 和内核参数。

### 直接使用 RustSBI 启动 Asterinas

返回 `workshop` 目录，运行下面命令：

```shell
$ cd ..
$ qemu-system-riscv64 \
-machine virt -cpu rv64,svpbmt=true,zkr=true -nographic -m 2G -smp 1 -no-reboot \
-bios ./rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
-kernel ./asterinas/boot/aster-nix-boot.elf \
-initrd ./asterinas/boot/initramfs-boot.cpio.gz \
-append "$(cat asterinas/boot/cmdline.txt)"
```

QEMU 的 CPU 配置需要开启 `Svpbmt` 和 `Zkr` 扩展。QEMU 加载 ELF 内核和 initramfs，并通过设备树传递内核参数及 initramfs 的位置。

终端输出 `Successfully booted.` 时，表示 Asterinas 已进入用户态并完成启动测试。

### 使用 RustSBI 和 U-Boot 启动 Asterinas

本小节在 `workshop` 目录下操作，使用前面生成的动态固件、Asterinas 内核和 initramfs。

#### 编译 U-Boot SPL

进入 U-Boot 目录

```shell
$ cd u-boot
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

本小节使用二进制文件 `spl/u-boot-spl` 和 `u-boot.itb`。RustSBI 动态固件通过 `OPENSBI` 打包到 `u-boot.itb`，由 U-Boot SPL 加载。

#### 准备 Asterinas 启动镜像

Asterinas 内核使用 ELF 格式，没有 Linux Image 启动头。需要将内核转换为 uImage，使用 U-Boot 的 `bootm` 命令启动。

进入启动文件目录，使用 `readelf` 和 GNU awk 从 ELF 中读取加载地址、入口地址和加载段的内存大小：

```shell
$ cd ../asterinas/boot
$ read load_addr entry_addr image_size < <(
LC_ALL=C riscv64-linux-gnu-readelf -hlW aster-nix-boot.elf | gawk '
/Entry point address:/ { entry = $4 }
$1 == "LOAD" {
    addr = strtonum($4)
    end = addr + strtonum($6)
    if (count == 0 || addr < base) base = addr
    if (end > limit) limit = end
    count++
}
END {
    if (count == 0 || entry == "") exit 1
    printf "0x%x %s %.0f\n", base, entry, limit - base
}'
)
```

转换内核，并补齐 `.bss` 等未初始化数据区所需的空间，再封装为 uImage：

```shell
$ riscv64-linux-gnu-objcopy -O binary aster-nix-boot.elf asterinas-kernel.bin
$ truncate -s "$image_size" asterinas-kernel.bin
$ ../../u-boot/tools/mkimage -A riscv -O linux -T kernel -C none \
-a "$load_addr" -e "$entry_addr" -n Asterinas \
-d asterinas-kernel.bin asterinas.uimg
```

创建 FAT 启动盘，并写入内核和 initramfs。此步骤无需挂载镜像：

```shell
$ truncate -s 64M asterinas-boot.img
$ mkfs.vfat -F 32 asterinas-boot.img
$ mcopy -i asterinas-boot.img asterinas.uimg ::asterinas.uimg
$ mcopy -i asterinas-boot.img initramfs-boot.cpio.gz ::initramfs.cpio.gz
```

#### 启动 Asterinas

返回 `workshop` 目录，生成稍后需要在 U-Boot 中执行的内核参数设置命令：

```shell
$ cd ../..
$ printf "setenv bootargs '%s'\n" "$(cat asterinas/boot/cmdline.txt)"
```

复制输出的命令，然后运行 QEMU。出现 `Hit any key to stop autoboot` 时按任意键，中断自动启动：

```shell
$ qemu-system-riscv64 \
-machine virt -cpu rv64,svpbmt=true,zkr=true -nographic -m 2G -smp 1 -no-reboot \
-bios ./u-boot/spl/u-boot-spl \
-device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
-blockdev driver=file,filename=./asterinas/boot/asterinas-boot.img,node-name=hd0 \
-device virtio-blk-device,drive=hd0
```

在 U-Boot 的 `=>` 提示符后粘贴并执行生成的 `setenv bootargs` 命令，再执行以下命令：

```shell
=> load virtio 0 ${kernel_addr_r} asterinas.uimg
=> load virtio 0 ${ramdisk_addr_r} initramfs-boot.cpio.gz
=> setenv rd_size ${filesize}
=> fdt addr ${fdtcontroladdr}
=> fdt move ${fdtcontroladdr} ${fdt_addr_r} 0x10000
=> bootm ${kernel_addr_r} ${ramdisk_addr_r}:${rd_size} ${fdt_addr_r}
```

U-Boot 将内核和 initramfs 加载到内存，并向内核传递设备树副本。终端输出 `Successfully booted.` 时，表示 Asterinas 已进入用户态并完成启动测试。
