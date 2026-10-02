# StarFive VisionFive2 开发板

[StarFive VisionFive2](https://doc-en.rvspace.org/VisionFive2/Datasheet/VisionFive_2/hardware.html) 是基于 JH7110 芯片的四核 64 位 RISC-V 单板计算机。

本节介绍使用 RustSBI 和 U-Boot 在 VisionFive2 中启动系统的基本流程。

## 烧录预编译固件

VisionFive2 可以直接使用集成了 RustSBI Prototyper 和 U-Boot 的[预编译固件 `rustsbi_visionfive2_fw_payload.img`](https://github.com/rustsbi/rustsbi/releases/download/v0.0.0-oscomp-2020/rustsbi_visionfive2_fw_payload.img)，无需下载源码或编译。该镜像写入板载 Flash 的 U-Boot 固件区域。

> **警告：烧录前必须备份板载 Flash 中的 SPL 和 U-Boot 固件，并确认备份可用。烧录步骤错误或写入过程中断电，可能导致开发板无法启动（变砖）。**

下载镜像，并使用支持 XMODEM 的串口终端，按照以下步骤烧录：

1. **连接串口**：在开发板断电时，将 USB 转串口模块的 RX 接到 Pin 8（UART TX），TX 接到 Pin 10（UART RX），GND 接到 Pin 6。引脚位置参见[官方引脚表](https://doc-en.rvspace.org/VisionFive2/Datasheet/VisionFive_2/gpio_pin_assig.html)。
2. **设置启动模式**：将启动拨码设为 [UART 模式](https://doc.rvspace.org/VisionFive2/Quick_Start_Guide/VisionFive2_SDK_QSG/boot_mode_settings.html)，即 `(RGPIO_1, RGPIO_0) = (1,1)`，串口波特率设为 `115200` bps。
3. **加载恢复程序**：给开发板上电，待终端连续显示 `C` 后，通过 XMODEM 发送官方[恢复程序目录](https://github.com/starfive-tech/Tools/tree/master/recovery)中的 `jh7110-recovery-20230322.bin`。
4. **烧录 RustSBI 固件**：进入恢复菜单后，输入 `2` 并按回车，按提示通过 XMODEM 发送 `rustsbi_visionfive2_fw_payload.img`，等待写入完成。
5. **从 Flash 启动**：关闭电源，将启动拨码恢复为 Flash 模式 `(RGPIO_1, RGPIO_0) = (0,0)`，重新上电，通过串口查看 RustSBI 和 U-Boot 的启动信息。

如果 Flash 为空或 SPL 已损坏，应在烧录 RustSBI 固件前，先在恢复菜单中输入 `0`，通过 XMODEM 烧录适用于该开发板的官方 `u-boot-spl.bin.normal.out`，再执行第 4 步。完整流程参见[官方串口烧录说明](https://doc.rvspace.org/VisionFive2/Quick_Start_Guide/VisionFive2_SDK_QSG/recovering_bootloader%20-%20vf2.html)。

## 自行编译与烧录固件

本教程使用软件版本如下：

|         软件         |  版本  |
| :------------------: | :----: |
|         GCC          | 10.5.0 |
| RustSBI Prototyper    | 0.0.0  |

### 准备工作

创建工作目录并进入该目录

```shell
$ mkdir workshop && cd workshop
```

#### 下载 VisionFive2 Debian 镜像

> 这里使用 Debian 镜像提供分区表。如果需要启动其他操作系统，可以在进入 U-Boot 后自行启动。

前往 <https://debian.starfivetech.com/> 下载 Debian 镜像，并将文件放入 `workshop` 目录。

假设下载的文件名为 `debian_image.img.bz2`，解压镜像：

```shell
$ bzip2 -dk debian_image.img.bz2
```

#### Clone VisionFive2 SDK

```shell
$ git clone -b JH7110_VisionFive2_devel https://github.com/starfive-tech/VisionFive2.git
$ cd VisionFive2
$ git submodule update --init --recursive
```

#### Clone RustSBI

在 VisionFive2 SDK 目录中下载 RustSBI

```shell
$ git clone https://github.com/rustsbi/rustsbi.git
```

### 编译 SDK 和 RustSBI

编译 SDK，编译产物位于 `work` 目录下。

```shell
$ make -j$(nproc)
```

进入 RustSBI 目录，安装构建工具。Rust 工具链及组件由 `rust-toolchain.toml` 指定，Rustup 会按需安装。

```shell
$ cd rustsbi
$ rustup show
$ cargo install --locked cargo-binutils --version 0.4.0
```

VisionFive2 的 SPL 将固件加载到 `0x40000000`，U-Boot 的链接地址为 `0x40200000`。复制默认配置，设置固件和 Payload 的地址：

```shell
$ cp firmware/prototyper/config/default.toml visionfive2.toml
$ sed -i \
-e 's/^link_start_address = .*/link_start_address = 0x40000000/' \
-e 's/^payload_address = .*/payload_address = 0x40200000/' visionfive2.toml
```

编译 RustSBI Prototyper，以 U-Boot 作为 Payload

```shell
$ cargo prototyper build --config-file visionfive2.toml \
--fdt ../work/u-boot/arch/riscv/dts/starfive_visionfive2.dtb \
payload ../work/u-boot/u-boot.bin
```

### 生成 Payload 镜像

返回 VisionFive2 SDK 目录

```shell
$ cd ..
```

创建 `payload_image.its`：

```plain
/dts-v1/;

/ {
	description = "U-boot-spl FIT image for JH7110 VisionFive2";
	#address-cells = <2>;

	images {
		firmware {
			description = "u-boot";
			data = /incbin/("./rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.bin");
			type = "firmware";
			arch = "riscv";
			os = "u-boot";
			load = <0x0 0x40000000>;
			entry = <0x0 0x40000000>;
			compression = "none";
		};
	};

	configurations {
		default = "config-1";

		config-1 {
			description = "U-boot-spl FIT config for JH7110 VisionFive2";
			firmware = "firmware";
		};
	};
};
```

使用 SDK 编译得到的 `mkimage` 生成镜像，并将其放入 `workshop` 目录：

```shell
$ ./work/u-boot/tools/mkimage -f payload_image.its -A riscv -O u-boot -T firmware ../visionfive2_fw_payload.img
```

### 烧录 Debian 镜像

返回 `workshop` 目录，将 Debian 镜像写入 SD 卡或 eMMC。假设设备路径为 `/dev/sda`，使用 `dd` 工具进行烧写：

**本命令仅为参考，请根据自己的磁盘路径修改。**

```shell
$ cd ..
$ sudo dd if=./debian_image.img of=/dev/sda status=progress conv=fsync
```

### 烧录 Payload 镜像

VisionFive2 支持从 1-bit QSPI NOR Flash、SDIO 3.0 和 eMMC 启动，不同启动模式的烧录方式有所不同。

可以按照[昉·星光 2 启动模式设置](https://doc.rvspace.org/VisionFive2/SDK_Quick_Start_Guide/VisionFive2_SDK_QSG/boot_mode_settings.html)修改启动模式。

#### 从 Flash 中启动

按照[烧录预编译固件](#烧录预编译固件)一节的串口流程，将自行编译得到的 `visionfive2_fw_payload.img` 写入 Flash 中的 U-Boot 固件区域。

#### 从 eMMC 或 SD 卡中启动

> VisionFive2 官方文档建议使用 QSPI Flash 启动，其他启动方式可能出现启动失败。

确保 SD 卡或 eMMC 按照[启动地址分配](https://doc.rvspace.org/VisionFive2/Developing_and_Porting_Guide/JH7110_Boot_UG/JH7110_SDK/boot_address_allocation.html)进行分区。

如果尚未分区，可以先完成[烧录 Debian 镜像](#烧录-debian-镜像)一节，使磁盘具有所需的分区表。

将 `visionfive2_fw_payload.img` 写入第 2 个分区：

**本命令仅为参考，请根据自己的磁盘路径修改。**

```shell
$ sudo dd if=./visionfive2_fw_payload.img of=/dev/sda2 status=progress conv=fsync
```

## 参考资料

- [昉·星光 2 SDK 快速参考手册](https://doc.rvspace.org/VisionFive2/SDK_Quick_Start_Guide/index.html)
- [昉·惊鸿-7110 启动手册](https://doc.rvspace.org/VisionFive2/Developing_and_Porting_Guide/JH7110_Boot_UG/)
