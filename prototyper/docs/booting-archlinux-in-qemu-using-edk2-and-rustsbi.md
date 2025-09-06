# 使用RustSBI和EDK2在QEMU中启动Arch Linux

本教程将介绍如何使用RustSBI和EDK2在QEMU中启动Arch Linux。

运行本教程需要在安装了Arch Linux x86_64的系统上进行。

## 创建根文件系统

首先创建一个`rootfs`文件夹并修改权限为`root`。

```bash
mkdir rootfs
sudo chown root:root ./rootfs
```

然后使用`pacstrap`这个`pacman`的初始化工具在`rootfs`安装`base`软件包，最好也顺便装一个`vim`。

```bash
sudo pacstrap \
	-C /usr/share/devtools/pacman.conf.d/extra-riscv64.conf \
	-M ./rootfs \
	base vim
sudo cp /usr/share/devtools/pacman.conf.d/extra-riscv64.conf rootfs/etc/
```

> `extra-riscv64.conf`是在`archlinuxcn/devtools-riscv64`软件包中提供的便利工具，
> 其中包括了`archriscv`该移植的`pacman.conf`文件，当然一般推荐修改一下该文件的镜像站点，以提高安装的速度。

然后清理一下`pacman`的缓存文件，缩小`rootfs`的大小，尤其是考虑到后面因为各种操作失误可能会反复解压`rootfs`文件。

```bash
sudo pacman  \
	--sysroot ./rootfs \
	--sync --clean --clean
```

然后设置一下该`rootfs`的`root`账号密码：

```bash
sudo usermod --root $(realpath ./rootfs) --password $(openssl passwd -6 "$password") root
```

就可以将`rootfs`打包为压缩包文件备用了。

```bash
sudo bsdtar --create \
    --auto-compress --options "compression-level=9"\
    --xattrs --acls\
    -f archriscv-rootfs.tar.zst -C rootfs/ .
```

## 初始化虚拟机镜像

首先，创建一个`qcow2`格式的QEMU虚拟机磁盘镜像：

```bash
qemu-img create -f qcow2 rustsbi-edk2-archriscv.img 10G
```

其中磁盘的大小可以自行定义。

为了能够像正常的磁盘一样进行读写，需要将该文件映射到一个块设备，而这通过`qemu-nbd`程序实现。首先需要加载该程序需要使用的内核驱动程序：

```bash
sudo modprobe nbd max_part=8
```

命令中的`max_part`指定了最多能够挂载的块设备（文件）个数。然后将该文件虚拟化为一个块设备：

```bash
sudo qemu-nbd -c /dev/nbd0 rustsbi-edk2-archriscv.img
```

挂载完毕之后就可以进行初始化虚拟机磁盘镜像的工作了。初始化虚拟机镜像主要涉及到如下几步：

- 格式化磁盘并安装根文件系统；
- 编译内核和生成初始化RAM磁盘。

使用EDK2进行引导需要磁盘的分区方式符合UEFI规范的要求，即使用`GPT`作为分区表的格式，并创建一个ESP(EFI System Parition)分区
存放启动系统。

首先使用`fdisk`工具进行格式化，这里生成的分区如下表所示。

| 分区          | 类型        | 格式  | 挂载点 | 大小       |
| ------------ | ---------- | ------ | ----- | ------ | 
| /dev/nbd0p1  | EFI System |FAT32     | /boot  | 512M       |
| /dev/nbd0p2  | Linux Filesystem |EXT4      | /      | 余下的空间 |

在使用`fdisk`完成硬盘的分区之后，进行分区的格式化。

```bash
sudo mkfs.fat -F 32 /dev/nbd0p1
sudo mkfs.ext4 /dev/nbd0p2
```

格式化完成之后，创建一个新的`mnt`目录，用于挂载新创建的硬盘。

```bash
sudo mkdir mnt
sudo mount /dev/nbd0p2 mnt
sudo mkdir mnt/boot
sudo mount /dev/nbd0p1 mnt/boot
```

将上一步中创建的根文件系统解压到`mnt`文件夹中：

```bash
cd mnt
sudo bsdtar -kpxf ../archriscv-rootfs.tar.zst
```

### 编译Linux RISC-V内核

这里不能使用Arch RISC-官方打包的Linux镜像，因为官方打包的镜像进行了压缩，不符合UEFI启动的标准，无法使用UEFI直接启动。

> 使得Linux符合UEFI标准的功能称作[Linux EFI STUB](https://docs.kernel.org/admin-guide/efi-stub.html)

这里使用Linux源代码自行编译内核。

```bash
wget https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.15.9.tar.xz
tar xvf linux-6.15.9.tar.xz
cd linux-6.15.9
ARCH=riscv make CROSS_COMPILE=riscv64-linux-gnu- defconfig
ARCH=riscv make CROSS_COMPILE=riscv64-linux-gnu- Image -j$(nproc)
```
编译完成之后的`arch/riscv/boot/Image`就是一个符合UEFI规范要求的EFI应用程序，将它复制到`mnt/boot`目录中。

```bash
cd ..
sudo cp linux-6.15.9/arch/riscv/boot/Image mnt/boot/linux-6.15.9.elf
```

编译Linux内核模块并安装到rootfs中。

```bash
cd linux-6.15.9
ARCH=riscv make CROSS_COMPILE=riscv64-linux-gnu- modules -j$(nproc)
sudo ARCH=riscv make CROSS_COMPILE=riscv64-linux-gnu- modules_install INSTALL_MOD_PATH=../mnt/usr
```

### 生成初始化RAM磁盘

使用如下的命`systemd-nspawn切换进行rootfs文件系统中。

```bash
sudo systemd-nspawn -D mnt /bin/bash
```

> 这也是在系统启动失败时进入系统进行修复的常用命令，采用`qemu-riscv64-static`实现跨架构二进制文件执行。

首先安装`mkinitcpio`，这是Arch Linux推荐的RAM磁盘生成工具。

```bash
pacman -Sy mkinitcpio
```

在`/etc/mkinitcpio.d/linux-6.15.9.preset`中复制如下的内容：

```
# mkinitcpio preset file for the 'linux' package

#ALL_config="/etc/mkinitcpio.conf"
ALL_kver="/boot/linux-6.15.9.elf"

PRESETS=('default')

#default_config="/etc/mkinitcpio.conf"
default_image="/boot/initramfs-6.15.9.img"
#default_uki="/efi/EFI/Linux/arch-linux.efi"
#default_options="--splash /usr/share/systemd/bootctl/splash-arch.bmp"
```

生成启动使用`initramfs`。

```bash
mkinitcpio -P
```

退出`systemd-nspawn`。

### 生成启动脚本

首先查看镜像中启动根文件系统的UUID。

```bash
export uuid=$(sudo findmnt mnt -o uuid -n)
```

在`mnt/boot`中生成启动脚本

```bash
cd mnt/boot
echo "\linux-6.15.9.elf initrd=initramfs-6.15.9.img rw root=UUID=${uuid} rootwait console=ttyS0,115200" | sudo tee startup.nsh
```

取消对于硬盘的挂载。

```bash
sudo umount -R mnt
sudo qemu-nbd -d /dev/nbd0
```

## 编译RustSBI和EDK2

首先拉取RustSBI的源代码并编译RustSBI：

```bash
git clone https://github.com/rustsbi/rustsbi.git --depth 1
cd rustsbi
cargo prototyper
```

然后拉取EDK2的源代码并编译EDK2:

```bash
git clone --recurse-submodule git@github.com:tianocore/edk2.git -b edk2-stable202505
cd edk2
export GCC5_RISCV64_PREFIX=riscv64-linux-gnu-
source edksetup.sh
build -a RISCV64 -b RELEASE -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC5
```

为了符合QEMU对于PFLASH固件的要求，将编译好的UEFI固件大小扩展到32M。

```bash
truncate -s 32M Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_VARS.fd 
truncate -s 32M Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_CODE.fd
```

## 启动QEMU

使用如下的指令启动QEMU：

```bash
qemu-system-riscv64  \
        -M virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
        -m 4096 -smp 8 \
        -bios rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin \
        -blockdev node-name=pflash0,driver=file,read-only=on,filename=edk2/Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_CODE.fd  \
        -blockdev node-name=pflash1,driver=file,filename=edk2/Build/RiscVVirtQemu/RELEASE_GCC5/FV/RISCV_VIRT_VARS.fd \
        -device virtio-blk-device,drive=hd0  \
        -drive file=rustsbi-edk2-archriscv.img,format=qcow2,id=hd0,if=none \
        -netdev user,id=n0 -device virtio-net,netdev=n0 \
        -monitor unix:/tmp/qemu-monitor,server,nowait \
        -nographic \
        -serial mon:stdio
```

启动系统并登录之后，验证是否使用UEFI启动成功：

```bash
$ cat /sys/firmware/efi/fw_platform_size 
64
```
