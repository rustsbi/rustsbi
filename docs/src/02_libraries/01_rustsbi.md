# `rustsbi` 核心抽象库

核心抽象库 `rustsbi` 提供封装 SBI 扩展的特型（trait）、生成宏和有关的辅助结构体、常量。我们首先介绍系统软件开发者如何引入 `rustsbi` 库；其次，我们介绍所有 `rustsbi` 导出结构的使用指南。

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

## SBI 扩展特型组合

`rustsbi` 包提供代表 SBI 扩展的特型（trait），而特型的实现对应着完整 SBI 扩展的实现。这种抽象方法统一了 RustSBI 生态对 SBI 扩展和实现的描述方式，它与平台无关，扩展了 RustSBI 的应用场景。

RustSBI 目前支持 RISC-V SBI 2.0 版本，具有以下扩展：DBCN（Console）、CPPC、RFNC（Fence）、HSM、IPI、NACL、PMU、SRST（Reset）、STA、SUSP和TIME（Timer）。

### DBCN（Console）调试控制台扩展

调试控制台扩展在 RISC-V SBI 中用于提供一个简单的文本输入、输出控制台，在 S 态操作系统驱动启动之前，临时充当早期控制台的作用，以辅助系统软件开发者的调试工作。

根据 RISC-V SBI 规范标准，建议使用 UTF-8 作为控制台扩展的操作字符集。

> 操作系统的驱动子系统启动后，不应再使用 SBI 调试控制台扩展。因此，SBI DBCN 扩展以兼容性为主要设计方向，不主要考虑高性能日志需求。
> 具有高性能日志需求的系统软件应当根据驱动子系统实现使用中断、DMA 等的高性能异步控制台，而不是使用 SBI DBCN 扩展。

> SBI DBCN 的用户无需添加硬件纠错位；若有必要，SBI 扩展的实现会自动处理硬件纠错逻辑。

本扩展的特型描述如下。

```rust
pub trait Console {
    // Required methods
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet;
    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet;
    fn write_byte(&self, byte: u8) -> SbiRet;
}
```

控制台扩展具有以下的函数。

#### 写字节串

```rust
fn write(&self, bytes: Physical<&[u8]>) -> SbiRet;
```

将字节串写入调试控制台。

写字节串操作是异步的。若调试控制台无法接受字节串的全部内容，它将可能仅写入部分的字节串，乃至不写入任何内容。
函数将返回实际写入调试控制台的字节串长度。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `bytes` | 输入字节串的物理地址段。它包括物理基地址的高、低部分，以及字节串的长度 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 写入成功，返回实际写入的字节串长度 |
| `SbiRet::invalid_param` | `bytes` 参数不满足物理地址段的要求，详见：物理地址段 |
| `SbiRet::denied` | 禁止写入调试控制台 |
| `SbiRet::failed` | 发生了 I/O 操作错误 |

#### 读字节串

```rust
fn read(&self, bytes: Physical<&[u8]>) -> SbiRet;
```

从调试控制台读取字节串。

读字节串操作是异步的。如果调试控制台没有字节可供读取，它将不读取任何内容。
函数将返回从调试控制台实际读取的字节串长度。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `bytes` | 读取缓冲区的物理地址段。它包括物理基地址的高、低部分，以及字节串的长度 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回实际读取的字节串长度 |
| `SbiRet::invalid_param` | `bytes` 参数不满足物理地址段的要求，详见：物理地址段 |
| `SbiRet::denied` | 禁止从调试控制台读取 |
| `SbiRet::failed` | 发生了 I/O 操作错误 |

#### 写单个字节

```rust
fn write_byte(&self, byte: u8) -> SbiRet;
```

将单个字节写入调试控制台。

单个字节写入是同步操作。若调试控制台无法写入，函数将阻塞，直到控制台能够写入单个字节。
或者，当 I/O 操作错误，返回 `SbiRet::failed` 也是允许的。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

> 写单个字节的操作无需构建物理地址段缓冲区，有助于支持为教育、实验而设计的简单的系统软件。
> 具有基本性能要求的工业级系统软件，通常将使用能够写字节串的 `write` 函数而不是此函数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `byte` | 需要写入调试控制台的单个字节 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回 0 |
| `SbiRet::denied` | 禁止从调试控制台读取 |
| `SbiRet::failed` | 发生了 I/O 操作错误 |

### CPPC 扩展

SBI CPPC 扩展应用于实现 ACPI 的系统固件。允许通过 SBI 调用访问系统软件环境中的 CPPC 寄存器。

> ACPI 代表“高级配置和电源管理接口”（Advanced Configuration and Power Management Interface），而 CPPC 代表“协作处理器性能控制”（Collaborative Processor Performance Control）。
>
> 对不使用 ACPI 的系统环境，固件可不实现此扩展。

CPPC 寄存器定义于 ACPI 标准规范中。每个 CPPC 寄存器都具有 32 位的寄存器编号；寄存器的宽度可能是 32 位或 64 位。许多 CPPC 寄存器是可读写的，也有一些 CPPC 寄存器是只读的。

> CPPC 规范保留定义只写权限的可能性，但目前还没有 CPPC 寄存器被定义为只写的。

一些 CPPC 寄存器编号是保留的。探测、读取和写入保留的 CPPC 寄存器将返回错误。

本扩展的特型描述如下。

```rust
pub trait Cppc {
    // Required methods
    fn probe(&self, reg_id: u32) -> SbiRet;
    fn read(&self, reg_id: u32) -> SbiRet;
    fn read_hi(&self, reg_id: u32) -> SbiRet;
    fn write(&self, reg_id: u32, val: u64) -> SbiRet;
}
```

CPPC 扩展具有以下的函数。

#### 探测 CPPC 寄存器

```rust
fn probe(&self, reg_id: u32) -> SbiRet;
```

探测对应的 CPPC 寄存器是否已经被当前平台实现。

如果此寄存器已被实现，返回它的宽度。若未被实现，返回 0。

> CPPC 寄存器的位宽度可能是 32 或者 64；因此，寄存器已被实现时，函数将以 32 或 64 作为返回值。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `reg_id` | 需要探测的 CPPC 寄存器编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 探测成功，返回宽度或者 0 |
| `SbiRet::invalid_param` | 该寄存器是保留的 CPPC 寄存器 |
| `SbiRet::failed` | 探测过程中发生了未指定的错误 |

#### 读取 CPPC 寄存器

```rust
fn read(&self, reg_id: u32) -> SbiRet;
```

读取 CPPC 寄存器的值。

若特权态（S 态）的位宽为 32，则仅返回目标寄存器值的低 32 位。

> 在 RV32 平台下，应当与 `read_hi` 函数组合，以完整读取 64 位 CPPC 寄存器的内容。

> 当 CPPC 寄存器未被当前平台实现时，将返回错误。若系统软件不确定平台是否实现了此寄存器，请先使用 `probe` 函数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `reg_id` | 需要读取的 CPPC 寄存器编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回目标寄存器的值 |
| `SbiRet::invalid_param` | 该寄存器是保留的 CPPC 寄存器 |
| `SbiRet::not_supported` | 该寄存器未被当前平台实现 |
| `SbiRet::denied` | CPPC 寄存器是只写的，禁止读取 |
| `SbiRet::failed` | 读取过程中发生了未指定的错误 |

#### 读取 CPPC 寄存器的高 32 位（仅限RV32）

```rust
fn read_hi(&self, reg_id: u32) -> SbiRet;
```

读取 CPPC 寄存器值的高 32 位。

若特权态（S 态）的位宽大于 32（如：64 位），则返回 0。

> 在 RV64 平台下，仅使用 `read` 函数即可完整读取 64 位寄存器的内容，无需配合使用 `read_hi` 函数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `reg_id` | 需要读取的 CPPC 寄存器编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回目标寄存器的值 |
| `SbiRet::invalid_param` | 该寄存器是保留的 CPPC 寄存器 |
| `SbiRet::not_supported` | 该寄存器未被当前平台实现 |
| `SbiRet::denied` | CPPC 寄存器是只写的，禁止读取 |
| `SbiRet::failed` | 读取过程中发生了未指定的错误 |

#### 写 CPPC 寄存器

```rust
fn write(&self, reg_id: u32, val: u64) -> SbiRet;
```

将新的值写入 CPPC 寄存器。

> 当 CPPC 寄存器未被当前平台实现时，将返回错误。若系统软件不确定平台是否实现了此寄存器，请先使用 `probe` 函数。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `reg_id` | 需要写入的 CPPC 寄存器编号 |
| `val` | 新的 CPPC 寄存器值 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 写入成功，返回 0 |
| `SbiRet::invalid_param` | 该寄存器是保留的 CPPC 寄存器 |
| `SbiRet::not_supported` | 该寄存器未被当前平台实现 |
| `SbiRet::denied` | CPPC 寄存器是只读的，禁止写入 |
| `SbiRet::failed` | 写入过程中发生了未指定的错误 |

### RFNC 远程栅栏扩展

远程栅栏扩展允许系统软件基于缓存同步等需求，指示其它核执行特定的同步指令。

可以执行的同步指令包括 `FENCE.I`、`SFENCE.VMA`。存在虚拟化 H 扩展的前提下，允许执行 `HFENCE.GVMA` 和 `HFENCE.VVMA` 同步指令。

同步指令可能由基地址、区间长度约束刷新范围。

> 具体来说，当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新完整的 TLB 快表缓存。

> 开发 SBI 固件实现时，若 `size` 规定的刷新区间过长，需要执行过多条带地址参数的刷新指令时，固件实现可以转而仅执行一条不带地址参数的完整刷新栅栏指令，以减少刷新指令的执行个数。

在部分远程栅栏操作中，`asid` 和 `vmid` 参数可用于指定需刷新的地址空间编号或虚拟机编号。

> 注意 `asid` 为零和不指定 `asid` 是不同的。以 `SFENCE.VMA` 为例，执行 `remote_sfence_vma` 表示不指定 `asid`，而刷新所有地址空间的快表缓存；而 `remote_sfence_vma_asid` 将以 `asid` 参数指定要刷新的地址空间。前者相当于 `sfence.vma vaddr, x0`；后者相当于 `sfence.vma vaddr, asid`，其中 `asid` 不等于 `x0`。

运用 `HartMask` 结构，系统软件可选择一次性刷新多个处理器核。

> `HartMask` 结构用于 `hart_mask` 参数，可为远程栅栏函数选择一个或多个处理器核。它的定义详见下文。

本扩展的特型描述如下。

```rust
pub trait Fence {
    // Required methods
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet;
    fn remote_sfence_vma(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
    ) -> SbiRet;
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet;

    // Provided methods
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet { ... }
    fn remote_hfence_gvma(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
    ) -> SbiRet { ... }
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet { ... }
    fn remote_hfence_vvma(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
    ) -> SbiRet { ... }
}
```

RFNC 扩展具有以下的函数。

#### 远程执行 `FENCE.I` 指令

```rust
fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet;
```

在对应的远程核上执行 `FENCE.I` 指令。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的处理器核 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程执行发送成功，返回 0 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的处理器核是 S 态中不可使用的 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

#### 远程执行 `SFENCE.VMA` 指令

```rust
fn remote_sfence_vma(
    &self,
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
) -> SbiRet;
```

在对应的远程核上执行 `SFENCE.VMA` 指令，以刷新所有的地址空间中 `start_addr` 和 `size` 规定的虚拟地址段。

> 相当于在对应的核上执行 `sfence.vma vaddr, x0` 或 `sfence.vma x0, x0` 指令。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，即执行 `sfence.vma x0, x0` 指令；否则，表示刷新部分地址空间，即执行多个 `sfence.vma vaddr, x0` 指令。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的处理器核 |
| `start_addr` | 虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程执行发送成功，返回 0 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的处理器核是 S 态中不可使用的 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

#### 指定地址空间，远程执行 `SFENCE.VMA` 指令

```rust
fn remote_sfence_vma_asid(
    &self,
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    asid: usize,
) -> SbiRet;
```

在对应的远程核上执行 `SFENCE.VMA` 指令，以刷新 `asid` 规定的地址空间中，`start_addr` 和 `size` 规定的虚拟地址段。

> 相当于在对应的核上执行 `sfence.vma x0, asid` 或 `sfence.vma vaddr, asid` 指令。

> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `sfence.vma x0, asid` 指令。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的处理器核 |
| `start_addr` | 虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |
| `asid` | 规定刷新操作生效的地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程执行发送成功，返回 0 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的处理器核是 S 态中不可使用的 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

#### 指定虚拟机编号，远程执行 `HFENCE.GVMA` 指令

```rust
fn remote_hfence_gvma_vmid(
    &self,
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    vmid: usize,
) -> SbiRet { ... }
```

在对应的远程核上执行 `HFENCE.GVMA` 指令，以刷新 `vmid` 规定的虚拟机编号中，`start_addr` 和 `size` 规定的客户机物理地址段。

> 相当于在对应的核上执行 `hfence.gvma x0, vmid` 或 `hfence.gvma gaddr, vmid` 指令。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.gvma x0, vmid` 指令。

`start_addr` 应为实际物理地址向右移动 2 位。

> 代表客户机起始物理地址时，`start_addr`（对应指令中的 `gaddr`）应为实际物理地址向右移动 2 位，因为 RISC-V 在开启虚拟内存的情况下，允许物理地址位数超过虚拟地址位数 2 位。因此，远程执行的刷新指令必须要求实际物理地址以 4 字节对齐。

函数只有在所有目标核都支持虚拟化 H 扩展时生效。

> 若对应的远程核至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的处理器核 |
| `start_addr` | 向右移动 2 位后的物理地址段的起始地址 |
| `size` | 物理地址段的长度，不需要向右移动 2 位。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |
| `vmid` | 规定刷新操作生效的虚拟机编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程执行发送成功，返回 0 |
| `SbiRet::not_supported` | 至少有一个被 `hart_mask` 选中的处理器核不支持虚拟化 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的处理器核是 S 态中不可使用的 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

#### 远程执行 `HFENCE.GVMA` 指令

```rust
fn remote_hfence_gvma(
    &self,
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
) -> SbiRet { ... }
```

在对应的远程核上执行 `HFENCE.GVMA` 指令，以刷新所有的虚拟机编号中 `start_addr` 和 `size` 规定的客户机物理地址段。

> 相当于在对应的核上执行 `hfence.gvma x0, x0` 或 `hfence.gvma gaddr, x0` 指令。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.gvma x0, x0` 指令。

`start_addr` 应为实际物理地址向右移动 2 位。

> 代表客户机起始物理地址时，`start_addr`（对应指令中的 `gaddr`）应为实际物理地址向右移动 2 位，因为 RISC-V 在开启虚拟内存的情况下，允许物理地址位数超过虚拟地址位数 2 位。因此，远程执行的刷新指令必须要求实际物理地址以 4 字节对齐。

函数只有在所有目标核都支持虚拟化 H 扩展时生效。

> 若对应的远程核至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的处理器核 |
| `start_addr` | 向右移动 2 位后的物理地址段的起始地址 |
| `size` | 物理地址段的长度，不需要向右移动 2 位。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程执行发送成功，返回 0 |
| `SbiRet::not_supported` | 至少有一个被 `hart_mask` 选中的处理器核不支持虚拟化 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的处理器核是 S 态中不可使用的 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

#### 指定地址空间，远程执行 `HFENCE.VVMA` 指令

```rust
fn remote_hfence_vvma_asid(
    &self,
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    asid: usize,
) -> SbiRet { ... }
```

在对应的远程核上执行 `HFENCE.VVMA` 指令，以刷新当前虚拟机内 `asid` 规定的地址空间中，`start_addr` 和 `size` 规定的虚拟地址段。

> 相当于在对应的核上执行 `hfence.vvma x0, asid` 或 `hfence.vvma vaddr, asid` 指令。当前虚拟机编号可由 `hgatp.VMID` CSR 寄存器位域获得。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.vvma x0, asid` 指令。

函数只有在所有目标核都支持虚拟化 H 扩展时生效。

> 若对应的远程核至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的处理器核 |
| `start_addr` | 虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |
| `asid` | 规定刷新操作生效的地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程执行发送成功，返回 0 |
| `SbiRet::not_supported` | 至少有一个被 `hart_mask` 选中的处理器核不支持虚拟化 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的处理器核是 S 态中不可使用的 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

#### 远程执行 `HFENCE.VVMA` 指令

```rust
fn remote_hfence_vvma(
    &self,
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
) -> SbiRet { ... }
```

在对应的远程核上执行 `HFENCE.VVMA` 指令，以刷新当前虚拟机内所有地址空间中，`start_addr` 和 `size` 规定的虚拟地址段。

> 相当于在对应的核上执行 `hfence.vvma x0, x0` 或 `hfence.vvma vaddr, x0` 指令。当前虚拟机编号可由 `hgatp.VMID` CSR 寄存器位域获得。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.vvma x0, x0` 指令。

函数只有在所有目标核都支持虚拟化 H 扩展时生效。

> 若对应的远程核至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数不具有返回值，因而 `SbiRet::success` 永远返回 0。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的处理器核 |
| `start_addr` | 虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程执行发送成功，返回 0 |
| `SbiRet::not_supported` | 至少有一个被 `hart_mask` 选中的处理器核不支持虚拟化 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的处理器核是 S 态中不可使用的 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

### HSM 多核管理扩展

多核管理扩展允许特权态（S 态）系统软件改变处理器核的运行状态。

本扩展的特型描述如下。

```rust
pub trait Hsm {
    // Required methods
    fn hart_start(
        &self,
        hartid: usize,
        start_addr: usize,
        opaque: usize,
    ) -> SbiRet;
    fn hart_stop(&self) -> SbiRet;
    fn hart_get_status(&self, hartid: usize) -> SbiRet;

    // Provided method
    fn hart_suspend(
        &self,
        suspend_type: u32,
        resume_addr: usize,
        opaque: usize,
    ) -> SbiRet { ... }
}
```

HSM 扩展具有以下的函数。

#### 启动处理器核

```rust
fn hart_start(
    &self,
    hartid: usize,
    start_addr: usize,
    opaque: usize,
) -> SbiRet;
```

启动对应处理器核到特权态（S 态）。

启动处理器核到特权态后，处理器核的初始寄存器内容如下。

| 寄存器 | 值 |
|:------|:----|
| `satp` | 0 |
| `sstatus.SIE` | 0 |
| `a0` | `hartid` |
| `a1` | `opaque` 参数内容 |

> 使用 HSM 扩展启动处理器核时，寄存器内容和通常的 SBI 启动流程一致。

处理器核启动操作是异步的。只要 SBI 实现确保返回代码正确，启动处理器核函数可以在对应启动操作开始执行之前返回。

若 SBI 实现是机器态（M 态）运行的 SBI 固件，则控制流转移到特权态（S 态）之前，它必须配置支持的任何物理内存保护机制（例如，PMP 定义的保护机制）和其它机器态（M 态）模式的状态。

目标核必须具有 `start_addr` 地址指令的执行权限，否则返回错误。

> 当物理内存保护机制（PMP 机制）或虚拟化 H 扩展的全局阶段（G 阶段）禁止此地址的执行操作时，`start_addr` 地址指令不具有执行权限，本函数应当返回 `SbiRet::invalid_address` 错误。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hartid` | 需要启动的处理器核编号 |
| `start_addr` | 处理器核启动时程序指针指向的物理地址 |
| `opaque` | 处理器核启动时，`a1` 寄存器包含的值 |

> 函数的 `start_addr` 参数表示物理地址，但是并不像通常的 SBI 扩展那样将物理地址分为高、低两部分。因为此时 `satp` 寄存器的内容（0）意味着内存管理单元（MMU）的虚拟内存功能一定被关闭，此时物理地址与虚拟地址的位宽相同。

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 启动成功，对应核心将从 `start_addr` 开始运行 |
| `SbiRet::invalid_address` | `start_addr` 无效，因为：它不是合法的物理地址，或物理内存保护机制（PMP 机制）、虚拟化 H 扩展的全局阶段（G 阶段）禁止此地址的执行操作 |
| `SbiRet::invalid_param` | `hartid` 无效，它不能启动到特权态（S 态） |
| `SbiRet::already_available` | `hartid` 对应的核已经启动 |
| `SbiRet::failed` | 启动过程中发生了未指定的错误 |

#### 停止处理器核

```rust
fn hart_stop(&self) -> SbiRet;
```

停止当前的处理器核。

> 处理器核停止后，其所有权交还到 SBI 实现。

停止处理器核函数执行成功后，函数不应当返回。

必须在特权态（S 态）中断关闭的情况下停止当前处理器核。

> 出于此 RISC-V SBI 规范标准的约定，SBI 实现不检查特权态中断是否已经关闭。

本函数没有传入参数。

函数可能返回以下错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::failed` | 停止过程中发生了未指定的错误 |

> 停止处理器核函数是特殊的，它不会返回 `SbiRet::success`。

#### 获取处理器核的运行状态

```rust
fn hart_get_status(&self, hartid: usize) -> SbiRet;
```

返回 `hartid` 处理器核的运行状态。

> 由于任何并发的 `hart_start`、`hart_stop` 或 `hart_suspend` 操作，处理器核的运行状态可能随时切换。因此，函数的返回值无法代表实时的处理器核的运行状态。

> SBI 实现中，应尽可能返回最新的处理器核运行信息，以减少系统软件中异步原语的调用次数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hartid` | 需要获取状态的处理器核编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 获取成功，返回对应处理器核的运行状态 |
| `SbiRet::invalid_param` | `hartid` 无效 |

#### 暂停处理器核

### IPI 核间中断扩展

### NACL 嵌套虚拟化加速扩展

### PMU 性能监测扩展

### SRST（Reset）系统重置扩展

### STA 丢失时间扩展

### SUSP 系统挂起扩展

### TIME（Timer）时钟扩展

## `EnvInfo` 特型

## 结构体

## 辅助常量

## 宏 `#[derive(RustSBI)]`
