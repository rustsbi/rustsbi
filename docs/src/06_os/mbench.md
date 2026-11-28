# mbench 测试与基准内核

`mbench`（仓库子模块 `firmware/mbench`，上游 `https://github.com/rustsbi/mbench`）是用于在 RISC-V 监管态调用并验证 SBI v3.0 接口与固件运行时行为的裸机测试与基准程序。启动后它会自动向串口打印 `@@MBENCH:*` 开头的输出，由 `cargo prototyper test` 与 `cargo prototyper bench` 调用 `mbench validate` 校验执行结果。

本节以 QEMU `virt` 平台为例，说明直接使用 RustSBI、以及通过 U-Boot 启动 `mbench` 的流程。

## 在 QEMU 中启动 mbench

本教程包含两种启动方式：

1. 直接由 RustSBI Prototyper 启动 `mbench`（包含 `payload`、`dynamic` 和 `jump` 三种模式）。
2. 由 RustSBI Prototyper 先启动 U-Boot，再在 U-Boot 中加载并启动 `mbench`。

### 获取源码与初始化子模块

首次获取 RustSBI 时，运行：

```shell
mkdir workshop && cd workshop
git clone -b main https://github.com/rustsbi/rustsbi.git
cd rustsbi
```

在 RustSBI 仓库根目录执行以下命令，首次检出或更新到 mbench 的最新 `main`：

```shell
git submodule update --init --remote -- firmware/mbench
```

省略 `--remote` 可以检出当前 RustSBI 提交记录的 mbench 版本。

自动 CI 在「Resolve mbench/main」中检出一次上游 `main`。同一轮 QEMU、Docker 和 Miralis 测试共用这次检出的提交。单独手动运行 Docker 或 Miralis 时，工作流会在「Checkout mbench」中检出当时的 `main`。要在本地复现，把检出步骤日志中的完整 SHA 赋给 `MBENCH_SHA`，并在对应的 RustSBI 版本下运行以下命令。

```shell
git -C firmware/mbench fetch origin "$MBENCH_SHA"
git -C firmware/mbench switch --detach FETCH_HEAD
```

### 在仓库根目录运行与构建

在仓库根目录检查所需工具并构建固件（完整的构建与运行参数见 `cargo prototyper test --help` / `bench --help`）：

```shell
rustup show
cargo install --locked cargo-binutils --version 0.4.0
cargo prototyper build
```

`cargo prototyper build` 会生成动态模式固件 `target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf`，独立固件编译不依赖初始化 `firmware/mbench` 子模块。

在仓库根目录直接以 `dynamic` 模式运行测试并保存串口日志到 `qemu.log`：

```shell
cargo prototyper test --mode dynamic --out qemu.log
```

若只需编译而不运行 QEMU，可追加 `--no-run`；产物位于 `target/mbench/test-0x80200000/mbench.{elf,bin}`。

### 配合 U-Boot 启动 mbench

当通过 U-Boot `booti` 把裸机内核加载到 `0x84000000` 时，`mbench-target` 的链接地址需要与实际加载地址一致。在仓库根目录通过环境变量指定加载地址与默认参数，编译并转换为原始二进制镜像：

```shell
MBENCH_LOAD_ADDR=0x84000000 MBENCH_DEFAULT_BOOTARGS="mode=check" \
  cargo build --locked --manifest-path firmware/mbench/Cargo.toml \
  --target-dir target/mbench-cargo \
  --package mbench-target --target riscv64imac-unknown-none-elf --release
rust-objcopy -O binary \
  target/mbench-cargo/riscv64imac-unknown-none-elf/release/mbench-target \
  target/mbench-0x84000000.bin
```

在编译好 `qemu-riscv64_smode_defconfig` 的 U-Boot 后，返回 `workshop` 目录启动 QEMU 并把镜像加载到 `0x84000000`：

```shell
cd ..
qemu-system-riscv64 \
  -machine virt -nographic -m 256M -smp 1 \
  -bios ./rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
  -kernel ./u-boot/u-boot.bin \
  -device loader,file=./rustsbi/target/mbench-0x84000000.bin,addr=0x84000000
```

在 U-Boot 的 `=>` 提示符后执行以下命令启动 `mbench`：

```shell
=> booti 0x84000000 - ${fdtcontroladdr}
```

测试完成后，`mbench` 会输出 `@@MBENCH:END` 汇总帧并通过 SBI SRST 请求正常关机。
