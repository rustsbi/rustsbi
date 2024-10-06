# 使用RustSBI & U-Boot在QEMU中启动 Fedora

本教程给出了使用RustSBI和U-Boot在QEMU中启动 Fedora 的基本流程。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
| riscv64-linux-gnu-gcc | 14.1.0  |
|  qemu-system-riscv64  |  9.0.1  |
|  RustSBI Prototyper   |  0.0.0  |
|        U-Boot         | 2024.04 |

## 准备RustSBI Prototyper， U-Boot ，Fedora 

创建工作目录并进入该目录

``` shell
$ mkdir workshop && cd workshop
```

### Clone RustSBI Prototyper

``` shell
$ git clone https://github.com/rustsbi/prototyper.git && cd prototyper && git checkout main && cd ..
```

### Clone U-Boot

``` shell
$ git clone https://github.com/u-boot/u-boot.git && cd u-boot && git checkout v2024.04 && cd ..
```
### 下载 Fedora 镜像文件

下载链接：<https://dl.fedoraproject.org/pub/alt/risc-v/disk_images/Fedora-40/Fedora.riscv64-40-20240429.n.0.qcow2>
```shell
$ mkdir -p fedora
$ cd fedora
$ wget https://dl.fedoraproject.org/pub/alt/risc-v/disk_images/Fedora-40/Fedora.riscv64-40-20240429.n.0.qcow2
$ cd ..
```

## 编译RustSBI  Prototyper

进入prototyper目录

``` shell
$ cd prototyper
```

编译RustSBI  Prototyper

``` shell
$ cargo make prototyper
```

## 编译U-Boot SPL

进入U-Boot目录

``` shell
$ cd u-boot
```

导出环境变量

```shell
$ export ARCH=riscv
$ export CROSS_COMPILE=riscv64-linux-gnu-
$ export OPENSBI=../prototyper/target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper.bin 
```

生成`.config`文件,编译U-Boot

```shell
# To generate .config file out of board configuration file
$ make qemu-riscv64_spl_defconfig
$ sed -i.bak 's/CONFIG_BOOTCOMMAND=*/CONFIG_BOOTCOMMAND="fatload virtio 0:1 84000000 EFI\/Linux\/6.8.7-300.4.riscv64.fc40.riscv64.efi; setenv bootargs root=UUID=57cbf0ca-8b99-45ae-ae9d-3715598f11c4 ro rootflags=subvol=root rhgb LANG=en_US.UTF-8 console=ttyS0 earlycon=sbi; bootefi 0x84000000 - ${fdtcontroladdr};"/' .config
$ make -j$(nproc)
```

## 配置 cloud-init

```shell
$ touch network-config
$ touch meta-data
$ cat >user-data <<EOF

#cloud-config
password: password
chpasswd:
  expire: False
ssh_pwauth: True
EOF

genisoimage \
    -output seed.img \
    -volid cidata -rational-rock -joliet \
    user-data meta-data network-config
```

## 使用RustSBI 原型系统和U-Boot启动 Fedora

进入`workshop`目录

``` shell
$ cd workshop
```

运行下面命令

``` shell
$ qemu-system-riscv64 \
    -nographic -machine virt \
    -smp 4 -m 8G \
    -bios ./u-boot/spl/u-boot-spl  \
    -device loader,file=./u-boot/u-boot.itb,addr=0x80200000 \
    -drive file=./fedora/Fedora.riscv64-40-20240429.n.0.qcow2,format=qcow2,if=none,id=hd0 \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-vga \
    -device virtio-rng-device,rng=rng0 \
    -device virtio-blk-device,drive=hd0 \
    -device virtio-net-device,netdev=usernet \
    -netdev user,id=usernet,hostfwd=tcp::12055-:22 \
    -device qemu-xhci -usb -device usb-kbd -device usb-tablet \
    -cdrom ./seed.img
```

帐号默认为 `fedora`，密码应为 `cloud-init` 配置过程中的 `password` 项值。
