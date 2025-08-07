# 使用RustSBI & U-Boot在QEMU中启动 PolyOS

尽管 Openharmony 在 4.1 引入了 `device_qemu-riscv64-linux`，但是目前仍无法按照[文档](https://gitee.com/openharmony/device_qemu/tree/HEAD/riscv64_virt#)正常编译。

本文于此介绍基于 OpenHarmony 的系统 -- PolyOS 使用 RustSBI 和 U-Boot 在 QEMU 中启动的方法。

下令 `$workdir` 表示工作目录。

### Clone & Compile RustSBI Prototyper

```shell
$ cd $workdir
$ git clone -b main https://github.com/rustsbi/rustsbi.git
```

```shell
$ cd rustsbi
```

编译RustSBI Prototyper

```shell
$ cargo prototyper
```

### Clone & Compile U-Boot

```shell
$ cd $workdir
$ git clone -b v2024.04 https://github.com/u-boot/u-boot.git
```

进入U-Boot目录

```shell
$ cd u-boot
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
$ export OPENSBI=../rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin
```

生成`.config`文件,编译U-Boot

```shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
$ make -j$(nproc)
```

### Download & Configure PolyOS

下载 PolyOS Mobile 镜像：<https://polyos.iscas.ac.cn/downloads/polyos-mobile-latest.img.tar.xz>。

```shell
$ cd $workdir
$ wget https://polyos.iscas.ac.cn/downloads/polyos-mobile-latest.img.tar.xz
$ tar xvf polyos-mobile-latest.img.tar.xz
```

创建一个带分区表的镜像，并创建一个分区。

```shell
$ cd ./image
$ qemu-img create boot.img 1g
$ fdisk boot.img
# 创建 GPT 分区表
> g
# 新建一个分区
> n
# 保存
> w
```

（ fdisk 的提示全选是和默认项即可。）

挂载本地回环设备：

```shell
$ sudo losetup --find --show -P ./boot.img
```

以下假设挂载的本地回环设备为 `/dev/loop1`。

将给定的 boot.ext4 写入该分区：

```shell
$ dd if=./boot.ext4 of=/dev/loop1p1
```

挂载该分区：

```shell
$ mkdir boot
$ mount /dev/loop1p1 ./boot
```

创建 `./boot/extlinux/extlinux.conf`，并写入以下内容：

```shell
default polyOS-RISC-V
label   polyOS-RISC-V
    kernel /Image
    initrd /ramdisk.img
    append 'loglevel=1 ip=192.168.137.2:192.168.137.1:192.168.137.1:255.255.255.0::eth0:off sn=0023456789 console=tty0,115200 console=ttyS0,115200 init=/bin/init ohos.boot.hardware=virt root=/dev/ram0 rw ohos.required_mount.system=/dev/block/vdb@/usr@ext4@ro,barrier=1@wait,required ohos.required_mount.vendor=/dev/block/vdc@/vendor@ext4@ro,barrier=1@wait,required ohos.required_mount.sys_prod=/dev/block/vde@/sys_prod@ext4@ro,barrier=1@wait,required ohos.required_mount.chip_prod=/dev/block/vdf@/chip_prod@ext4@ro,barrier=1@wait,required ohos.required_mount.data=/dev/block/vdd@/data@ext4@nosuid,nodev,noatime,barrier=1,data=ordered,noauto_da_alloc@wait,reservedsize=1073741824 ohos.required_mount.misc=/dev/block/vda@/misc@none@none=@wait,required'
```

卸载相关分区和本地回环设备：

```shell
$ umount ./boot
$ losetup -d /dev/loop1
```

### USE Qemu to bootup

使用 qemu 启动：

```shell
$ cd $workdir/image
image_path=`pwd`
qemu-system-riscv64 \
    -name PolyOS-Mobile \
    -machine virt \
    -m 4096\
    -smp 4 \
    -no-reboot \
	-bios ../u-boot/spl/u-boot-spl \
	-device loader,file=../u-boot/u-boot.itb,addr=0x80200000 \
    -drive if=none,file=${image_path}/boot.img,format=raw,id=boot,index=6 \
	-device ahci,id=ahci -device ide-hd,bus=ahci.0,drive=boot \
    -drive if=none,file=${image_path}/updater.img,format=raw,id=updater,index=5 \
    -device virtio-blk-device,drive=updater \
    -drive if=none,file=${image_path}/system.img,format=raw,id=system,index=4 \
    -device virtio-blk-device,drive=system \
    -drive if=none,file=${image_path}/vendor.img,format=raw,id=vendor,index=3 \
    -device virtio-blk-device,drive=vendor \
    -drive if=none,file=${image_path}/userdata.img,format=raw,id=userdata,index=2 \
    -device virtio-blk-device,drive=userdata \
    -drive if=none,file=${image_path}/sys_prod.img,format=raw,id=sys-prod,index=1 \
    -device virtio-blk-device,drive=sys-prod \
    -drive if=none,file=${image_path}/chip_prod.img,format=raw,id=chip-prod,index=0 \
    -device virtio-blk-device,drive=chip-prod \
    -nographic \
    -device virtio-gpu-pci,xres=486,yres=864,max_outputs=1,addr=08.0 \
    -monitor telnet:127.0.0.1:55555,server,nowait \
    -device virtio-mouse-pci \
    -device virtio-keyboard-pci \
    -device es1370 \
    -k en-us \
    -display sdl,gl=off
```
