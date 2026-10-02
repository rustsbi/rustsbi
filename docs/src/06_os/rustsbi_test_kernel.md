# RustSBI 测试用内核

RustSBI 测试用内核是用于检查 SBI 接口及固件运行行为的测试程序。它使用 `sbi-testing` 等测试模块，在 RISC-V 内核态调用 SBI 接口，启动后自动运行测试并输出结果。

本节以 QEMU `virt` 平台为例，介绍直接使用 RustSBI，以及使用 RustSBI 和 U-Boot 启动测试用内核的方法。

## 在 QEMU 中启动 RustSBI 测试用内核

本教程给出了在 QEMU 中启动 RustSBI 测试用内核的基本流程，分为两种类型：

1. 直接使用 RustSBI 启动测试用内核。
2. 使用 RustSBI 和 U-Boot 启动测试用内核。

本教程使用软件版本如下：

|         软件         |        版本        |
| :------------------: | :----------------: |
| riscv64-linux-gnu-gcc |       16.2.0       |
|       Binutils       |        2.47        |
| qemu-system-riscv64   |       11.1.2       |
| Rust                 | 由 `rust-toolchain.toml` 指定 |
| RustSBI Prototyper    |       0.0.0        |
| U-Boot               |      2026.07       |

### 准备 RustSBI Prototyper、测试用内核和 U-Boot

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

在 Arch Linux 中安装交叉编译器、QEMU 和构建工具：

```shell
$ sudo pacman -S git base-devel riscv64-linux-gnu-gcc qemu-system-riscv openssl gnutls python python-setuptools python-pyelftools swig
```

在 Ubuntu 中安装交叉编译器、QEMU 和构建工具：

```shell
$ sudo apt install git build-essential gcc-riscv64-linux-gnu qemu-system-misc bison flex bc libssl-dev libgnutls28-dev python3-dev python3-setuptools python3-pyelftools swig
```

测试用内核需要 QEMU 9.1 或更新版本。若软件源提供的版本较旧，请先更新 QEMU。

#### Clone RustSBI

```shell
$ git clone -b main https://github.com/rustsbi/rustsbi.git
```

测试用内核随 RustSBI 源码一同提供，无需单独下载。

#### Clone U-Boot

仅在使用 RustSBI 和 U-Boot 启动测试用内核时需要下载 U-Boot。若直接使用 RustSBI 启动，可跳过本小节。

```shell
$ git clone -b v2026.07 https://github.com/u-boot/u-boot.git
```

### 编译 RustSBI Prototyper 和测试用内核

进入 RustSBI 目录

```shell
$ cd rustsbi
```

Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。

安装构建工具，编译 RustSBI Prototyper 和测试用内核

```shell
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
$ cargo prototyper build
$ cargo prototyper test --no-run
```

`--no-run` 只构建固件和测试用内核，跳过命令默认的 QEMU 测试。后续两种启动流程使用以下文件：

- 动态固件：`target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf`。
- 测试用内核：`target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin`。

### 直接使用 RustSBI 启动测试用内核

返回 `workshop` 目录，运行下面命令：

```shell
$ cd ..
$ qemu-system-riscv64 \
-machine virt -nographic -m 256M -smp 1 \
-bios ./rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
-kernel ./rustsbi/target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin
```

QEMU 加载动态固件和测试用内核，由 RustSBI 启动测试用内核。测试通过后，终端输出 `SBI tests completed: PASS`，测试用内核请求关机，QEMU 随后退出。

### 使用 RustSBI 和 U-Boot 启动测试用内核

本小节在 `workshop` 目录下操作，使用前面生成的动态固件和测试用内核。

#### 编译 U-Boot

进入 U-Boot 目录

```shell
$ cd u-boot
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
```

生成 `.config` 文件，编译 U-Boot

```shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_smode_defconfig
$ make -j$(nproc)
```

`qemu-riscv64_smode_defconfig` 用于构建运行于内核态的 U-Boot，由 RustSBI 提供 SBI 服务。本小节使用二进制文件 `u-boot.bin`。

#### 启动测试用内核

返回 `workshop` 目录，运行下面命令。出现 `Hit any key to stop autoboot` 时按任意键，中断自动启动：

```shell
$ cd ..
$ qemu-system-riscv64 \
-machine virt -nographic -m 256M -smp 1 \
-bios ./rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
-kernel ./u-boot/u-boot.bin \
-device loader,file=./rustsbi/target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin,addr=0x84000000
```

在 U-Boot 的 `=>` 提示符后执行以下命令，启动测试用内核：

```shell
=> booti 0x84000000 - ${fdtcontroladdr}
```

测试通过后，终端输出 `SBI tests completed: PASS`，测试用内核请求关机，QEMU 随后退出。
