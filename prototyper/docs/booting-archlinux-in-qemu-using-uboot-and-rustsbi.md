# 使用RustSBI & U-Boot在QEMU中启动ArchLinux

本教程给出了使用RustSBI和U-Boot在QEMU中启动ArchLinux的基本流程。

本教程要求您使用非RISC-V Arch Linux(x86_64 或 aarch64 等)机器上运行，因为我们使用了`pacstrap`和`pacman`。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
| riscv64-linux-gnu-gcc | 14.1.0  |
|  qemu-system-riscv64  |  9.0.1  |
|  RustSBI Prototyper   |  0.0.0  |
|        U-Boot         | 2024.04 |

1. 安装依赖环境。
``` shell
# pacman -Syu then reboot is recommended before this
$ sudo pacman -S arch-install-scripts git qemu-img qemu-system-riscv riscv64-linux-gnu-gcc devtools-riscv64
```

2. Clone构建脚本，构建rootfs和镜像。
``` shell
$ git clone -b rustsbi https://github.com/guttatus/archriscv-scriptlet.git
$ cd archriscv-scriptlet
$ ./mkrootfs
$ ./mkimg
```
3. 使用Qemu启动Archlinux
``` shell
$ ./startqemu.sh
```
如果在最后一步中，您发现自己卡在 `[ OK ] Reached target Graphical Interface` 超过5分钟，只需按 `Ctrl`-`C` 并重新运行 `startqemu.sh`。
