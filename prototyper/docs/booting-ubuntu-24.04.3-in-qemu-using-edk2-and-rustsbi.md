# 使用RustSBI和EDK2在QEMU中启动Ubuntu 24.04.3

本教程将介绍如何使用RustSBI和EDK2在QEMU中启动Ubuntu 24.04.3。

运行本教程已经在Arch Linux x86_64系统上经过测试，但理论上其他的Linux发行版亦可以参照使用。

先记录工作目录，后续命令均以此目录为基准：

```bash
export WORKSHOP_DIR=$(pwd)
```

测试本教程时使用的软件版本如下：

|          软件           |      版本       |
|:---------------------:|:-------------:|
| riscv64-linux-gnu-gcc |    15.1.0     |
|  qemu-system-riscv64  |    10.1.0     |
|  RustSBI Prototyper   |    master     |
|         EDK2          | stable202505  |

## 准备Ubuntu镜像

访问[Download Ubuntu for RISC-V](https://ubuntu.com/download/risc-v)页面，查找当前Ubuntu针对**QEMU Emulator**平台发行的
最新预安装镜像进行下载。

本教程撰写时的最新镜像版本为[24.04.3 LTS](https://cdimage.ubuntu.com/releases/24.04.3/release/ubuntu-24.04.3-preinstalled-server-riscv64.img.xz)，复制链接进行下载：

```bash
wget https://cdimage.ubuntu.com/releases/24.04.3/release/ubuntu-24.04.3-preinstalled-server-riscv64.img.xz
xz -d ubuntu-24.04.3-preinstalled-server-riscv64.img.xz
```

## 编译RustSBI和EDK2

首先拉取RustSBI的源代码并编译RustSBI：

```bash
git clone https://github.com/rustsbi/rustsbi.git --depth 1 "$WORKSHOP_DIR/rustsbi"
(
    cd "$WORKSHOP_DIR/rustsbi"
    cargo prototyper
)
```

然后拉取EDK2的源代码并编译EDK2:

```bash
git clone --recurse-submodules https://github.com/tianocore/edk2.git -b edk2-stable202505 "$WORKSHOP_DIR/edk2"
cd "$WORKSHOP_DIR/edk2"
export GCC5_RISCV64_PREFIX=riscv64-linux-gnu-
source edksetup.sh
build -a RISCV64 -b RELEASE -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC5
```

为了符合QEMU对于PFLASH固件的要求，将编译好的UEFI固件大小扩展到32M。

```bash
truncate -s 32M Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_VARS.fd 
truncate -s 32M Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_CODE.fd
```

## 启动系统

使用如下的指令启动QEMU：

```bash
qemu-system-riscv64  \
        -M virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
        -m 4096 -smp 8 \
        -bios "$WORKSHOP_DIR/rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin" \
        -blockdev node-name=pflash0,driver=file,read-only=on,filename="$WORKSHOP_DIR/edk2/Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_CODE.fd" \
        -blockdev node-name=pflash1,driver=file,filename="$WORKSHOP_DIR/edk2/Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_VARS.fd" \
        -device virtio-blk-device,drive=hd0  \
        -drive file=ubuntu-24.04.3-preinstalled-server-riscv64.img,format=raw,id=hd0,if=none \
        -netdev user,id=n0 -device virtio-net,netdev=n0 \
        -monitor unix:/tmp/qemu-monitor,server,nowait \
        -nographic \
        -serial mon:stdio
```

启动系统的过程中需要注意，在默认情况下，EDK2启动之后会启动GRUB2作为引导程序，启动之后的默认账户和密码是`ubuntu`和`ubuntu`。

启动系统并登录之后，验证是否使用UEFI启动成功：

```bash
ubuntu@ubuntu:~$ cat /sys/firmware/efi/fw_platform_size 
64
```
