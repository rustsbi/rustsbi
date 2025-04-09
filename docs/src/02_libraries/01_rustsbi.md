# `rustsbi` 核心抽象库

核心抽象库 `rustsbi` 提供封装 SBI 扩展的特型（trait）、生成宏和有关的辅助结构体、常量。我们首先介绍系统软件开发者如何引入 `rustsbi` 库；其次，我们介绍所有的 `rustsbi` 代码成员。

## 引入 `rustsbi` 库作为依赖包

为了在使用 Cargo 的 Rust 项目中引入 `rustsbi` 库，应当将 `rustsbi` 添加到对应 `Cargo.toml` 文件的 `[dependencies]` 章节中。可以使用命令行或者手动修改文件的形式添加依赖库。同时，需要根据项目的特点选择需要的 Cargo 特性（features）。

开发 RISC-V 机器态裸机固件时，需要增加 `machine` 特性，以引入裸机下的 RISC-V 环境寄存器识别操作。需要运行以下指令。

```bash
cargo add rustsbi --features machine
```
```toml
# 或者，在Cargo.toml中添加以下内容（下文不再赘述）
[dependencies]
rustsbi = { version = "0.4.0", features = ["machine"] }
```

> 机器态裸机固件将识别的 RISC-V 环境寄存器例如 `mvendorid` 和 `mimpid`，因此只能在 RISC-V 目标下编译。
>
> 编译完成后，目标项目应当在机器态运行，以获得直接访问这些寄存器的权限。其它特权态的软件需获取环境寄存器内容时，应当调用 SBI 接口，而不是直接读取环境寄存器。

开发虚拟化软件时，有时需要转发使用宿主机环境本身具有的 SBI 接口，此时我们增加 `forward` 特性。

```bash
cargo add rustsbi --features forward
```
```toml
[dependencies]
rustsbi = { version = "0.4.0", features = ["forward"] }
```

> 虚拟化软件运行于 RISC-V 的 HS 态。无论是 Type-1 还是 Type-2 虚拟化软件，都需要为其中运行于 VS 态的客户机系统提供 SBI 接口。然而，HS 态宿主机系统也是运行在外部提供的 SBI 接口之上；因此可能存在 SBI 转发操作。
>
> 当 `forward` 特性开启时，`rustsbi` 库直接调用 HS 态所处的 SBI 环境，它将使用仅支持 RISC-V 架构的 `sbi-rt` 包，因此此选项只能在 RISC-V 编译目标下编译。

开发模拟器时，使用宿主 SBI 环境的需求不常见。因此，我们不增加任何特性地引入 `rustsbi` 。

```bash
cargo add rustsbi
```
```toml
[dependencies]
rustsbi = "0.4.0"
```

> 模拟器通常会提供自己的 `vendorid` 和 `impid` 等环境寄存器，以表示模拟器环境的产品标识符和版本号。不增加任何特性的 `rustsbi` 包可以在任何支持的平台下编译，不仅限于 RISC-V。
>
> `rustsbi` 包默认情况下不包含任何特性，即 default features 为空，不开启使用 RISC-V 汇编语言的代码特性，以适应软件测试的需求。

最新的 RustSBI 版本号是 0.4.0。使用以上的命令或配置，可以依赖于最新的 RustSBI 0.4.0 版本。然而，如果需要使用其它的 RustSBI 版本，应当在 `cargo add` 命令中增加依赖包的版本号。例如：

```bash
cargo add rustsbi@0.3.2
```

不同 RustSBI 版本的特性不同，应参考对应版本的 RustSBI 手册以获取这方面的帮助。

## 代码成员

### 特型（trait）组合

### 结构体

### 辅助常量
