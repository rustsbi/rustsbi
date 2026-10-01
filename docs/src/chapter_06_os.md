# 系统内核与发行版用户指南

本章介绍如何用 RustSBI 启动 RISC-V 内核和操作系统。教程主要使用 QEMU `virt`，有的直接用 RustSBI 启动内核，有的需要 U-Boot 或 EDK2 加载系统。

如果想自己编译内核，请看 [Linux 内核](06_os/linux_kernel.md)或 [Asterinas](06_os/asterinas.md) 的教程。要运行完整的系统，请阅读 [Arch Linux](06_os/arch_linux.md)、[Fedora](06_os/fedora.md)、[FreeBSD](06_os/freebsd.md)、[openEuler](06_os/openeuler.md)、[OpenWrt](06_os/openwrt.md)、[PolyOS](06_os/polyos.md) 和 [Ubuntu](06_os/ubuntu.md) 的教程。

[RustSBI 测试用内核](06_os/rustsbi_test_kernel.md)会在启动后自动运行 SBI 测试并输出结果，可以用来检查固件是否正常工作。

在开发板上烧录和启动固件的方法，见[平台用户指南](chapter_05_platform.md)。
