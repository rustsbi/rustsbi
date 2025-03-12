# 在 VisionFive2 中使用 RustSBI 和 U-Boot 启动

本教程给出了使用 RustSBI 和 U-Boot 在 VisionFive2 中启动的基本流程。

本教程使用软件版本如下：

|         软件          |  版本   |
| :-------------------: | :-----: |
|          gcc          | **10.5.0**  |
|  RustSBI Prototyper   |  0.0.0  |

## 准备工作

创建工作目录

``` shell
$ mkdir workshop 
```

### 下载 VisionFive2 Debian 镜像

前往 <https://debian.starfivetech.com/> 下载最新 Debian 镜像。

假设下载下来的文件名为 `debian_image.img.bz2`。

```shell
$ bzip2 -dk debian_image.img.bz2
```

移动 `debian_image.img` 到 `workshop` 目录下。

### Clone VisionFive2 SDK

```shell
$ cd workshop
$ git clone git@github.com:starfive-tech/VisionFive2.git
$ cd VisionFive2
$ git checkout JH7110_VisionFive2_devel
$ git submodule update --init --recursive
```

### Clone RustSBI

```shell
$ cd workshop/VisionFive2
$ git clone https://github.com/rustsbi/rustsbi
```

## 编译 SDK 和 RustSBI

编译 SDK，编译产物应在 `work` 目录下。

``` shell
$ cd workshop/VisionFive2
$ make -j$(nproc)
```


编译 RustSBI Prototyper，以 U-Boot 作为 Payload。

``` shell
$ cd workshop/VisionFive2/rustsbi 
$ cargo prototyper --payload ../work/u-boot/u-boot.bin --fdt ../work/u-boot/arch/riscv/dts/starfive_visionfive2.dtb 
```

## 生成 Payload 镜像

创建 `workshop/VisionFive2/payload_image.its`:

```plain
/dts-v1/;

/ {
	description = "U-boot-spl FIT image for JH7110 VisionFive2";
	#address-cells = <2>;

	images {
		firmware {
			description = "u-boot";
			data = /incbin/("./rustsbi/target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper-payload.bin");
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

生成 `visionfive2_fw_payload.img`：
```shell
$ cd workshop/VisionFive2
$ mkimage -f payload_image.its -A riscv -O u-boot -T firmware visionfive2_fw_payload.img
```

## 烧录 Payload 镜像

VisionFive2 支持从 1-bit QSPI Nor Flash、SDIO3.0、eMMC 中启动，对于不同启动模式，烧录方式有所不同。

你可以参照[ 昉·星光 2启动模式设置 ](https://doc.rvspace.org/VisionFive2/SDK_Quick_Start_Guide/VisionFive2_SDK_QSG/boot_mode_settings.html)来修改启动模式。

### 从 Flash 中启动

参照 [更新Flash中的SPL和U-Boot](https://doc.rvspace.org/VisionFive2/SDK_Quick_Start_Guide/VisionFive2_SDK_QSG/updating_spl_and_u_boot%20-%20vf2.html) 或 [恢复Bootloader](https://doc.rvspace.org/VisionFive2/SDK_Quick_Start_Guide/VisionFive2_SDK_QSG/recovering_bootloader%20-%20vf2.html) 来更新 Flash 中的 U-Boot 部分。

### 从 EMMC 或 SD 卡中启动

> 非 Flash 的启动方式并不被 VisionFive2 官方文档建议。

首先，你需要确保你的对应磁盘（如 SD 卡）按照[ 启动地址分配 -  昉·惊鸿-7110启动手册 ](https://doc.rvspace.org/VisionFive2/Developing_and_Porting_Guide/JH7110_Boot_UG/JH7110_SDK/boot_address_allocation.html)进行分区。

你可以先完成[烧录 Debian 镜像](#烧录-bebian-镜像)一节，这样你的对应磁盘的分区表就应已满足上述要求。

之后，将上文得到的 `visionfive2_fw_payload.img` 写入表中所指向的 2 号分区即可。

**本命令仅为参考，请根据自己的磁盘路径修改**
```shell
$ cd workshop
$ dd if=./visionfive2_fw_payload.img of=/dev/sda2 status=progress
```

## 烧录 Debian 镜像

假设你的对应磁盘路径为 `/dev/sda`，按如下命令使用 `dd` 工具进行烧写即可：

```shell
$ cd workshop
$ dd if=./debian_image.img of=/dev/sda
```

## SEE ALSO

- [昉·星光 2 SDK快速参考手册](https://doc.rvspace.org/VisionFive2/SDK_Quick_Start_Guide/index.html)
- [昉·惊鸿-7110启动手册](https://doc.rvspace.org/VisionFive2/Developing_and_Porting_Guide/JH7110_Boot_UG/)
