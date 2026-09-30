# `rustsbi` 核心抽象库

核心抽象库 `rustsbi` 提供封装 SBI 扩展的特型（trait）、派生宏和有关的辅助结构体、常量。我们首先介绍系统软件开发者如何引入 `rustsbi` 库；其次，我们介绍 `rustsbi` 主要公共接口的使用指南。

## 引入 `rustsbi` 库作为依赖包

为了在使用 Cargo 的 Rust 项目中引入 `rustsbi` 库，应当将 `rustsbi` 添加到对应 `Cargo.toml` 文件的 `[dependencies]` 章节中。可以使用命令行或者手动修改文件的形式添加依赖库。同时，需要根据项目的特点选择需要的 Cargo 特性（features）。

开发 RISC-V 机器态裸机固件时，可启用 `machine` 特性，以直接读取 RISC-V 机器态标识寄存器。需要运行以下指令。

```bash
cargo add rustsbi --features machine
```
```toml
# 或者，在Cargo.toml中添加以下内容（下文不再赘述）
[dependencies]
rustsbi = { version = "0.4.1", features = ["machine"] }
```

> `machine` 特性读取的机器态标识寄存器包括 `mvendorid`、`marchid` 和 `mimpid`，执行这些读取操作需要 RISC-V 目标和 M 态权限。
>
> 编译完成后，目标项目应当在机器态运行，以获得直接访问这些寄存器的权限。S 态或 VS 态软件需获取这些寄存器内容时，应当调用 SBI 接口，而不是直接读取环境寄存器。

开发虚拟化软件时，有时需要转发使用宿主机环境本身具有的 SBI 接口，此时我们增加 `forward` 特性。

```bash
cargo add rustsbi --features forward
```
```toml
[dependencies]
rustsbi = { version = "0.4.1", features = ["forward"] }
```

> 使用 H 扩展时，虚拟机监控程序运行于 RISC-V 的 HS 态，可为运行于 VS 态的客户机系统提供 SBI 接口。然而，HS 态宿主机系统也是运行在外部提供的 SBI 接口之上；因此可能存在 SBI 转发操作。
>
> 当 `forward` 特性开启时，`Forward` 可通过 `sbi-rt` 将选定的 SBI 调用转发给宿主执行环境。实际调用需要 RISC-V SBI 运行环境；非 RISC-V 目标上的编译仅用于测试等用途。

开发模拟器时，使用宿主 SBI 环境的需求不常见。因此，我们不增加任何特性地引入 `rustsbi`。

```bash
cargo add rustsbi
```
```toml
[dependencies]
rustsbi = "0.4.1"
```

> 模拟器可通过 `EnvInfo` 提供符合 `mvendorid`、`marchid` 和 `mimpid` 定义的环境标识信息。不增加任何特性的 `rustsbi` 包可以在任何支持的平台下编译，不仅限于 RISC-V。
>
> `rustsbi` 包默认情况下不包含任何特性，即 default features 为空，不开启使用 RISC-V 汇编语言的代码特性，以适应软件测试的需求。

以上依赖配置以 RustSBI 0.4.1 为例。如果需要使用其它的 RustSBI 版本，应当在 `cargo add` 命令中增加依赖包的版本号。例如：

```bash
cargo add rustsbi@0.3.2
```

不同 RustSBI 版本的特性不同，应参考对应版本的 RustSBI 手册以获取这方面的帮助。

## SBI 扩展特型组合

`rustsbi` 包提供代表 SBI 扩展的特型（trait），而特型的实现提供相应的 SBI 扩展功能。这种抽象方法统一了 RustSBI 生态对 SBI 扩展和实现的描述方式，它与平台无关，扩展了 RustSBI 的应用场景。

RustSBI 目前支持 RISC-V SBI 2.0 版本，提供包括以下扩展在内的接口：DBCN（Console）、CPPC、RFNC（Fence）、HSM、IPI、NACL、PMU、SRST（Reset）、STA、SUSP 和 TIME（Timer）。

此外，`rustsbi` 还提供 SBI 3.0 引入的 SSE、FWFT、DBTR 和 MPXY 扩展特型。扩展特型描述接口，具体功能是否可用由平台实现决定。

### DBCN（Console）调试控制台扩展

调试控制台扩展在 RISC-V SBI 中用于提供一个简单的文本输入、输出控制台，在 S 态操作系统驱动启动之前，临时充当早期控制台的作用，以辅助系统软件开发者的调试工作。

根据 RISC-V SBI 规范标准，建议使用 UTF-8 作为控制台扩展的字符编码。

> 操作系统的驱动子系统启动后，仍可按需使用 SBI 调试控制台扩展。DBCN 主要用于早期输出和调试。
> 具有高性能日志需求的系统软件可通过驱动子系统使用中断、DMA 等方式实现高性能控制台。

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

写字节串操作是非阻塞的。若调试控制台无法接受字节串的全部内容，它将可能仅写入部分的字节串，乃至不写入任何内容。
成功时，`SbiRet::value` 为实际写入调试控制台的字节数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `bytes` | 输入字节串的物理地址段。它包括物理基地址的高、低部分，以及字节串的长度 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 写入成功，返回实际写入的字节串长度 |
| `SbiRet::invalid_param` | `bytes` 参数不满足 SBI 规范的共享内存要求 |
| `SbiRet::denied` | 禁止写入调试控制台 |
| `SbiRet::failed` | 发生了 I/O 操作错误 |

#### 读字节串

```rust
fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet;
```

从调试控制台读取字节串。

读字节串操作是非阻塞的。如果调试控制台没有字节可供读取，它将不读取任何内容。
成功时，`SbiRet::value` 为从调试控制台实际读取的字节数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `bytes` | 读取缓冲区的物理地址段。它包括物理基地址的高、低部分，以及缓冲区的容量 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回实际读取的字节串长度 |
| `SbiRet::invalid_param` | `bytes` 参数不满足 SBI 规范的共享内存要求 |
| `SbiRet::denied` | 禁止从调试控制台读取 |
| `SbiRet::failed` | 发生了 I/O 操作错误 |

#### 写单个字节

```rust
fn write_byte(&self, byte: u8) -> SbiRet;
```

将单个字节写入调试控制台。

单个字节写入是阻塞操作。函数将等待单个字节写入完成。
若禁止输出或发生 I/O 错误，则返回相应错误。

本函数的 `SbiRet::value` 固定为 0，`SbiRet::error` 表示操作结果。

> 写单个字节的操作无需构建物理地址段缓冲区，有助于支持为教育、实验而设计的简单的系统软件。
> 批量输出时，通常使用 `write` 函数以减少 SBI 调用次数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `byte` | 需要写入调试控制台的单个字节 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 写入成功，`value` 为 0 |
| `SbiRet::denied` | 禁止写入调试控制台 |
| `SbiRet::failed` | 发生了 I/O 操作错误 |

### CPPC 扩展

SBI CPPC 扩展允许通过 SBI 调用访问系统软件环境中的 CPPC 寄存器。

> ACPI 代表“高级配置与电源接口”（Advanced Configuration and Power Interface），而 CPPC 代表“协作处理器性能控制”（Collaborative Processor Performance Control）。
>
> CPPC 驱动也可基于设备树实现，不要求系统采用 ACPI。

CPPC 寄存器主要依据 ACPI 规范定义，SBI 还补充定义了性能状态转换延迟等信息。每个 CPPC 寄存器都具有 32 位的寄存器编号；寄存器的宽度可能是 32 位或 64 位。许多 CPPC 寄存器是可读写的，也有一些 CPPC 寄存器是只读的。

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

> CPPC 寄存器的位宽可能是 32 或者 64；因此，寄存器已被实现时，函数将以 32 或 64 作为返回值。

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

若调用方 S 态的 XLEN 为 32，则仅返回目标寄存器值的低 32 位。

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

#### 读取 CPPC 寄存器的高 32 位

```rust
fn read_hi(&self, reg_id: u32) -> SbiRet;
```

读取 CPPC 寄存器值的高 32 位。

若调用方 S 态的 XLEN 为 64 或更大，则成功返回的 `SbiRet::value` 为 0。

> 在 RV64 平台下，仅使用 `read` 函数即可完整读取 64 位寄存器的内容，无需配合使用 `read_hi` 函数。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `reg_id` | 需要读取的 CPPC 寄存器编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回高 32 位；调用方 XLEN 为 64 或更大时为 0 |
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

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `reg_id` | 需要写入的 CPPC 寄存器编号 |
| `val` | 新的 CPPC 寄存器值 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 写入成功 |
| `SbiRet::invalid_param` | 该寄存器是保留的 CPPC 寄存器 |
| `SbiRet::not_supported` | 该寄存器未被当前平台实现 |
| `SbiRet::denied` | CPPC 寄存器是只读的，禁止写入 |
| `SbiRet::failed` | 写入过程中发生了未指定的错误 |

### RFNC 远程栅栏扩展

远程栅栏扩展允许系统软件基于缓存同步等需求，指示其它 hart 执行特定的同步指令。

可以执行的同步指令包括 `FENCE.I`、`SFENCE.VMA`。存在虚拟化 H 扩展的前提下，允许执行 `HFENCE.GVMA` 和 `HFENCE.VVMA` 同步指令。

同步指令可能由基地址、区间长度约束刷新范围。

> 具体来说，当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新所选 ASID 或 VMID 作用域内的完整地址范围。

> 开发 SBI 固件实现时，若 `size` 规定的刷新区间过长，需要执行过多条带地址参数的刷新指令时，固件实现可以转而仅执行一条不带地址参数的完整刷新栅栏指令，以减少刷新指令的执行个数。

在部分远程栅栏操作中，`asid` 和 `vmid` 参数可用于指定需刷新的地址空间编号或虚拟机编号。

> 注意 `asid` 为零和不指定 `asid` 是不同的。以 `SFENCE.VMA` 为例，执行 `remote_sfence_vma` 表示不指定 `asid`，而刷新所有地址空间的快表缓存；而 `remote_sfence_vma_asid` 将以 `asid` 参数指定要刷新的地址空间。前者相当于 `sfence.vma vaddr, x0`；后者相当于 `sfence.vma vaddr, asid`，其中保存 ASID 的源寄存器不是 `x0`，但其值可以为 0。

运用 `HartMask` 结构，系统软件可选择一次性刷新多个 hart。

> `HartMask` 结构用于 `hart_mask` 参数，可为远程栅栏函数选择一个或多个 hart。它封装了 SBI 的 hart 位掩码和起始编号。

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

在对应的远程 hart 上执行 `FENCE.I` 指令。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的 hart |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程栅栏请求发送成功 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的 hart 未由平台启用或对 S 态不可用 |
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

在对应的远程 hart 上执行 `SFENCE.VMA` 指令，以刷新所有的地址空间中 `start_addr` 和 `size` 规定的虚拟地址段。

> 相当于在对应的 hart 上执行 `sfence.vma vaddr, x0` 或 `sfence.vma x0, x0` 指令。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，即执行 `sfence.vma x0, x0` 指令；否则，表示刷新部分地址空间，可执行一条或多条 `sfence.vma vaddr, x0` 指令覆盖指定范围。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的 hart |
| `start_addr` | 虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程栅栏请求发送成功 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的 hart 未由平台启用或对 S 态不可用 |
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

在对应的远程 hart 上执行 `SFENCE.VMA` 指令，以刷新 `asid` 规定的地址空间中，`start_addr` 和 `size` 规定的虚拟地址段。

> 相当于在对应的 hart 上执行 `sfence.vma x0, asid` 或 `sfence.vma vaddr, asid` 指令。

> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `sfence.vma x0, asid` 指令。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的 hart |
| `start_addr` | 虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |
| `asid` | 规定刷新操作生效的地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程栅栏请求发送成功 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | `asid` 无效，或至少有一个被 `hart_mask` 选中的 hart 未由平台启用或对 S 态不可用 |
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

在对应的远程 hart 上执行 `HFENCE.GVMA` 指令，以刷新 `vmid` 规定的虚拟机编号中，`start_addr` 和 `size` 规定的客户机物理地址段。

> 相当于在对应的 hart 上执行 `hfence.gvma x0, vmid` 或 `hfence.gvma gaddr, vmid` 指令。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.gvma x0, vmid` 指令。

`start_addr` 为客户机物理字节地址，无需右移。

> 右移 2 位是 `HFENCE.GVMA` 指令地址操作数的编码要求，由 SBI 实现处理；SBI 调用的 `start_addr` 和 `size` 均以字节为单位。

函数只有在所有目标 hart 都支持虚拟化 H 扩展时生效。

> 若对应的远程 hart 至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的 hart |
| `start_addr` | 客户机物理地址段的起始字节地址 |
| `size` | 客户机物理地址段的字节数。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |
| `vmid` | 规定刷新操作生效的虚拟机编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程栅栏请求发送成功 |
| `SbiRet::not_supported` | 函数未实现，或至少有一个被 `hart_mask` 选中的 hart 不支持 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | `vmid` 无效，或至少有一个被 `hart_mask` 选中的 hart 未由平台启用或对 S 态不可用 |
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

在对应的远程 hart 上执行 `HFENCE.GVMA` 指令，以刷新所有的虚拟机编号中 `start_addr` 和 `size` 规定的客户机物理地址段。

> 相当于在对应的 hart 上执行 `hfence.gvma x0, x0` 或 `hfence.gvma gaddr, x0` 指令。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.gvma x0, x0` 指令。

`start_addr` 为客户机物理字节地址，无需右移。

> 右移 2 位是 `HFENCE.GVMA` 指令地址操作数的编码要求，由 SBI 实现处理；SBI 调用的 `start_addr` 和 `size` 均以字节为单位。

函数只有在所有目标 hart 都支持虚拟化 H 扩展时生效。

> 若对应的远程 hart 至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的 hart |
| `start_addr` | 客户机物理地址段的起始字节地址 |
| `size` | 客户机物理地址段的字节数。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程栅栏请求发送成功 |
| `SbiRet::not_supported` | 函数未实现，或至少有一个被 `hart_mask` 选中的 hart 不支持 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的 hart 未由平台启用或对 S 态不可用 |
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

在对应的远程 hart 上执行 `HFENCE.VVMA` 指令，以刷新当前虚拟机内 `asid` 规定的地址空间中，`start_addr` 和 `size` 规定的客户机虚拟地址段。

> 相当于在对应的 hart 上执行 `hfence.vvma x0, asid` 或 `hfence.vvma vaddr, asid` 指令。当前虚拟机编号由调用 hart 的 `hgatp.VMID` 字段指定。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.vvma x0, asid` 指令。

函数只有在所有目标 hart 都支持虚拟化 H 扩展时生效。

> 若对应的远程 hart 至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的 hart |
| `start_addr` | 客户机虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |
| `asid` | 规定刷新操作生效的地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程栅栏请求发送成功 |
| `SbiRet::not_supported` | 函数未实现，或至少有一个被 `hart_mask` 选中的 hart 不支持 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | `asid` 无效，或至少有一个被 `hart_mask` 选中的 hart 未由平台启用或对 S 态不可用 |
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

在对应的远程 hart 上执行 `HFENCE.VVMA` 指令，以刷新当前虚拟机内所有地址空间中，`start_addr` 和 `size` 规定的客户机虚拟地址段。

> 相当于在对应的 hart 上执行 `hfence.vvma x0, x0` 或 `hfence.vvma vaddr, x0` 指令。当前虚拟机编号由调用 hart 的 `hgatp.VMID` 字段指定。
>
> 当 `start_addr` 和 `size` 均为 0，或 `size` 等于 `usize::MAX` 时，表示刷新整个地址空间，执行 `hfence.vvma x0, x0` 指令。

函数只有在所有目标 hart 都支持虚拟化 H 扩展时生效。

> 若对应的远程 hart 至少有一个不支持虚拟化 H 扩展，实现应当返回 `SbiRet::not_supported` 错误。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要选中的 hart |
| `start_addr` | 客户机虚拟地址段的起始地址 |
| `size` | 虚拟地址段的长度。为 `usize::MAX`，或与 `start_addr` 均为 0 时，表示整个地址空间 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 远程栅栏请求发送成功 |
| `SbiRet::not_supported` | 函数未实现，或至少有一个被 `hart_mask` 选中的 hart 不支持 H 扩展 |
| `SbiRet::invalid_address` | `start_addr` 或 `size` 不合法 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的 hart 未由平台启用或对 S 态不可用 |
| `SbiRet::failed` | 远程栅栏发生了未指定的错误 |

### HSM 硬件线程状态管理扩展

硬件线程状态管理扩展允许特权态（S 态）系统软件请求改变硬件线程（hart）的运行状态。

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

#### 启动 hart

```rust
fn hart_start(
    &self,
    hartid: usize,
    start_addr: usize,
    opaque: usize,
) -> SbiRet;
```

启动对应 hart 到特权态（S 态）。

启动 hart 到特权态后，hart 的初始寄存器内容如下。

| 寄存器 | 值 |
|:------|:----|
| `satp` | 0 |
| `sstatus.SIE` | 0 |
| `a0` | `hartid` |
| `a1` | `opaque` 参数内容 |

> 除以上寄存器外，其它寄存器的初始状态未定义。

hart 启动操作是异步的。只要 SBI 实现确保返回代码正确，启动 hart 函数可以在目标 hart 开始执行之前返回。

若 SBI 实现是机器态（M 态）运行的 SBI 固件，则控制流转移到特权态（S 态）之前，它必须配置支持的任何物理内存保护机制（例如，PMP 定义的保护机制）和其它机器态（M 态）模式的状态。

目标 hart 必须具有 `start_addr` 地址指令的执行权限，否则返回错误。

> 当物理内存保护机制（PMP 机制）或虚拟化 H 扩展的 G 阶段地址转换禁止此地址的执行操作时，`start_addr` 地址指令不具有执行权限，本函数应当返回 `SbiRet::invalid_address` 错误。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hartid` | 需要启动的 hart 编号 |
| `start_addr` | hart 启动时程序指针指向的物理地址 |
| `opaque` | hart 启动时，`a1` 寄存器包含的值 |

> `start_addr` 只需使用单个 XLEN 位参数传递，因为目标 hart 在 `satp = 0` 的状态下开始执行 S 模式代码，入口地址必须能由 XLEN 位表示。这不意味着平台的物理地址位宽与虚拟地址位宽相同。

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 启动请求成功，对应 hart 将从 `start_addr` 开始运行 |
| `SbiRet::invalid_address` | `start_addr` 无效，因为：它不是合法的物理地址，或物理内存保护机制（PMP 机制）、虚拟化 H 扩展的 G 阶段地址转换禁止此地址的执行操作 |
| `SbiRet::invalid_param` | `hartid` 无效，它不能启动到特权态（S 态） |
| `SbiRet::already_available` | `hartid` 对应的 hart 已经启动 |
| `SbiRet::failed` | 启动过程中发生了未指定的错误 |

#### 停止 hart

```rust
fn hart_stop(&self) -> SbiRet;
```

停止当前的 hart。

> hart 停止后，其所有权交还到 SBI 实现。

停止 hart 函数执行成功后，函数不应当返回。

必须在特权态（S 态）中断关闭的情况下停止当前 hart。

> 关闭特权态中断是调用方必须满足的前提，不能依赖 SBI 实现代为检查。

本函数没有传入参数。

函数可能返回以下错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::failed` | 停止过程中发生了未指定的错误 |

> 停止 hart 函数是特殊的，它不会返回 `SbiRet::success`。

#### 获取 hart 的运行状态

```rust
fn hart_get_status(&self, hartid: usize) -> SbiRet;
```

返回编号为 `hartid` 的 hart 的运行状态。

> 由于任何并发的 `hart_start`、`hart_stop` 或 `hart_suspend` 操作，hart 的运行状态可能随时切换。因此，调用方检查返回值时，实际状态可能已经改变。

> SBI 实现中，应尽可能返回最新的 hart 运行信息，以反映查询时的 HSM 状态。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hartid` | 需要获取状态的 hart 编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 获取成功，返回对应 hart 的运行状态 |
| `SbiRet::invalid_param` | `hartid` 无效 |

#### 挂起 hart

```rust
fn hart_suspend(
    &self,
    suspend_type: u32,
    resume_addr: usize,
    opaque: usize,
) -> SbiRet { ... }
```

将当前 hart 置于 `suspend_type` 指定的挂起状态，以降低空闲时的功耗。收到中断或平台规定的硬件事件后，hart 将恢复运行。

挂起状态分为保留状态和非保留状态两类。保留状态保存各特权态的寄存器和 CSR 内容，恢复后从本函数成功返回；非保留状态不保存这些内容，恢复时将跳转到 `resume_addr` 指定的物理地址，在特权态（S 态）继续执行。

> 保留状态不使用 `resume_addr`。非保留状态需要软件保存并恢复所需的运行上下文，不能依赖原有的寄存器内容。

从非保留状态恢复时，hart 的寄存器内容如下。

| 寄存器 | 值 |
|:------|:----|
| `satp` | 0 |
| `sstatus.SIE` | 0 |
| `a0` | 当前 hart 的编号 |
| `a1` | `opaque` 参数内容 |

> 除以上寄存器外，其它寄存器的初始状态未定义。`resume_addr` 必须能由 XLEN 位表示，并允许恢复后的 S 态执行该地址的指令。

`suspend_type` 为 32 位整数，其取值如下。

| 值 | 说明 |
|:---|:-----|
| `0x0000_0000` | 默认保留状态，可使用 `sbi_spec::hsm::suspend_type::RETENTIVE` |
| `0x0000_0001` ..= `0x0FFF_FFFF` | 保留供未来使用 |
| `0x1000_0000` ..= `0x7FFF_FFFF` | 平台专用的保留状态 |
| `0x8000_0000` | 默认非保留状态，可使用 `sbi_spec::hsm::suspend_type::NON_RETENTIVE` |
| `0x8000_0001` ..= `0x8FFF_FFFF` | 保留供未来使用 |
| `0x9000_0000` ..= `0xFFFF_FFFF` | 平台专用的非保留状态 |

本函数具有默认实现，返回 `SbiRet::not_supported`。实现 HSM 扩展时，可根据平台的低功耗能力重写本函数。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `suspend_type` | 请求进入的挂起状态 |
| `resume_addr` | 从非保留状态恢复时，程序指针指向的物理地址 |
| `opaque` | 从非保留状态恢复时，`a1` 寄存器包含的值 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | hart 已进入保留状态，并成功恢复运行 |
| `SbiRet::invalid_param` | `suspend_type` 为保留值，或为未实现的平台专用状态 |
| `SbiRet::not_supported` | 函数未实现，或挂起状态已实现但平台缺少支持它的必要条件 |
| `SbiRet::invalid_address` | `resume_addr` 不是合法的物理地址，或物理内存保护机制、虚拟化 H 扩展的 G 阶段地址转换禁止 S 态执行该地址的指令 |
| `SbiRet::failed` | 挂起过程中发生了未指定的错误 |

> 从非保留状态成功恢复时，控制流转移到 `resume_addr`，本函数不会返回 `SbiRet::success`。从保留状态成功恢复时，建议返回 `SbiRet::success(0)`。

### IPI 核间中断扩展

核间中断扩展允许系统软件向一个或多个 hart 发送中断，用于通知其它 hart 处理待办工作。

接收方通过特权态（S 态）软件中断处理这些请求。系统软件可使用 `HartMask` 结构选择目标 hart。

本扩展的特型描述如下。

```rust
pub trait Ipi {
    // Required method
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet;
}
```

IPI 扩展具有以下的函数。

#### 发送核间中断

```rust
fn send_ipi(&self, hart_mask: HartMask) -> SbiRet;
```

向 `hart_mask` 选中的所有 hart 发送核间中断。

> 核间中断在目标 hart 上表现为 S 态软件中断。调用成功表示中断请求已发送，不表示目标 hart 已完成中断处理。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 需要接收核间中断的 hart，包含 hart 位掩码和起始编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 已向所有目标 hart 发送核间中断 |
| `SbiRet::invalid_param` | 至少有一个被 `hart_mask` 选中的 hart 未由平台启用，或对 S 态不可用 |
| `SbiRet::failed` | 发送过程中发生了未指定的错误 |

### NACL 嵌套虚拟化加速扩展

嵌套虚拟化允许虚拟机监控程序将另一个虚拟机监控程序作为客户机运行。外层虚拟机监控程序称为 L0，客户机中的虚拟机监控程序称为 L1。

NACL 扩展通过共享内存传递 H 扩展的 CSR 访问和 `HFENCE` 请求，使 L1 可以批量提交操作，由 L0 在同步时处理，以减少逐条指令陷入所产生的开销。

> 在硬件已实现 H 扩展的平台上，机器态（M 态）固件不应实现 NACL 扩展。本扩展主要用于外层虚拟机监控程序向客户机提供嵌套虚拟化加速。

使用 NACL 扩展前，系统软件应当为每个需要使用它的虚拟 hart 设置共享内存，并通过 `probe_feature` 探测所需功能。

本扩展的特型描述如下。

```rust
pub trait Nacl {
    // Required methods
    fn probe_feature(&self, feature_id: u32) -> SbiRet;
    fn set_shmem(&self, shmem: SharedPtr<[u8; NATIVE]>, flags: usize) -> SbiRet;
    fn sync_csr(&self, csr_num: usize) -> SbiRet;
    fn sync_hfence(&self, entry_index: usize) -> SbiRet;
    fn sync_sret(&self) -> SbiRet;
}
```

其中，`SharedPtr` 来自 `sbi_spec::binary`，`NATIVE` 来自 `sbi_spec::nacl::shmem_size`，表示与编译目标字长对应的共享内存大小。

> `sync_csr`、`sync_hfence` 和 `sync_sret` 对应的加速功能在 SBI 规范中是可选的，但这些方法在 Rust 的 `Nacl` 特型中没有默认实现。未提供对应功能时，实现应当返回 `SbiRet::not_supported`。

NACL 扩展具有以下的函数。

#### 探测嵌套虚拟化加速功能

```rust
fn probe_feature(&self, feature_id: u32) -> SbiRet;
```

探测 `feature_id` 指定的加速功能是否可用。

本函数成功返回，`SbiRet::value` 为 1 表示功能可用，为 0 表示功能不可用。

功能编号如下。

| 编号 | 常量 | 说明 |
|:----|:-----|:-----|
| 0 | `sbi_spec::nacl::feature_id::SYNC_CSR` | 同步共享内存中的 CSR |
| 1 | `sbi_spec::nacl::feature_id::SYNC_HFENCE` | 同步共享内存中的 `HFENCE` 请求 |
| 2 | `sbi_spec::nacl::feature_id::SYNC_SRET` | 同步共享内存并模拟 `SRET` 指令 |
| 3 | `sbi_spec::nacl::feature_id::AUTOSWAP_CSR` | 根据共享内存配置自动交换指定 CSR 的值 |

> 其它编号保留供未来使用。探测不可用的功能时，应返回 `SbiRet::success(0)`，而不是错误。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `feature_id` | 需要探测的加速功能编号，宽度为 32 位 |

函数可能返回以下内容：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 探测成功，返回 1 表示可用，返回 0 表示不可用 |

#### 设置嵌套虚拟化加速共享内存

```rust
fn set_shmem(&self, shmem: SharedPtr<[u8; NATIVE]>, flags: usize) -> SbiRet;
```

为当前 hart 设置并启用嵌套虚拟化加速共享内存，或关闭其加速功能。

共享内存的物理基地址必须按 4096 字节对齐，大小为 `4096 + 1024 * (XLEN / 8)` 字节。其中，前 4096 字节保存各加速功能使用的辅助数据，后续区域保存 1024 个 XLEN 位的 CSR 值。共享内存中的多字节数据使用小端字节序。

> XLEN 的单位为位。因此，RV32 对应 8192 字节，RV64 对应 12288 字节。`NATIVE` 的值随 Rust 编译目标的 `usize` 宽度确定。

SBI 实现必须检查共享内存的访问权限。RustSBI 的 `Nacl::set_shmem` 接口约定要求成功返回前将共享区域清零；系统软件应在设置成功后填入需要提交的 CSR、栅栏请求和恢复上下文。

> `shmem` 同时保存物理地址的高、低部分。传入 `SharedPtr::new(usize::MAX, usize::MAX)` 时，表示关闭当前 hart 的嵌套虚拟化加速功能，此时不访问对应地址。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `shmem` | 共享内存的物理基地址；高、低部分均为 `usize::MAX` 时关闭加速功能 |
| `flags` | 保留供未来使用，必须为 0 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 已设置共享内存，或已关闭加速功能 |
| `SbiRet::invalid_param` | `flags` 不为 0，或启用时共享内存的物理基地址未按 4096 字节对齐 |
| `SbiRet::invalid_address` | 共享内存不可写，或其物理地址范围不满足 SBI 共享内存的访问要求 |

#### 同步共享内存中的 CSR

```rust
fn sync_csr(&self, csr_num: usize) -> SbiRet;
```

同步共享内存与 SBI 实现维护的 H 扩展 CSR 状态。

调用方可在共享内存中写入新的 CSR 值，并设置对应的脏位。同步时，SBI 实现处理这些写入，清除脏位，并将最新 CSR 值写回共享内存。

> `csr_num` 为 `usize::MAX` 时，同步 SBI 实现支持的所有 H 扩展 CSR。否则，它应当是已实现的 CSR 编号，并满足 `(csr_num & 0x300) == 0x200` 且 `csr_num < 0x1000`。

本函数仅在 `SYNC_CSR` 功能可用时生效。调用前必须为当前 hart 设置 NACL 共享内存。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `csr_num` | 需要同步的 CSR 编号；为 `usize::MAX` 时同步所有已实现的 H 扩展 CSR |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | CSR 同步成功 |
| `SbiRet::not_supported` | `SYNC_CSR` 功能不可用 |
| `SbiRet::invalid_param` | `csr_num` 不是 `usize::MAX`，且不是已实现的有效 H 扩展 CSR 编号 |
| `SbiRet::no_shmem` | 当前 hart 的 NACL 共享内存不可用 |

#### 同步共享内存中的 `HFENCE` 请求

```rust
fn sync_hfence(&self, entry_index: usize) -> SbiRet;
```

处理共享内存中记录的嵌套 `HFENCE` 请求。

每个嵌套 `HFENCE` 请求条目的第一个字为配置字（`Config`），其最高位（第 `XLEN-1` 位）为待处理标志（`Pending`），规范将其记为 `Config.Pending`。

调用方填入请求内容后，将待处理标志置为 1。调用本函数时，SBI 实现仅处理所选条目中该标志为 1 的请求，并在处理完成后将其清零；该标志为 0 的条目会被跳过。

> `entry_index` 为 `usize::MAX` 时，同步所有条目。否则，它应当小于 `3840 / XLEN`；XLEN 为 32 时共有 120 个条目，为 64 时共有 60 个条目。

本函数仅在 `SYNC_HFENCE` 功能可用时生效。调用前必须为当前 hart 设置 NACL 共享内存。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `entry_index` | 需要同步的 `HFENCE` 条目编号，从 0 开始；为 `usize::MAX` 时同步所有条目 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | `HFENCE` 请求同步成功 |
| `SbiRet::not_supported` | `SYNC_HFENCE` 功能不可用 |
| `SbiRet::invalid_param` | `entry_index` 不是 `usize::MAX`，且不小于 `3840 / XLEN` |
| `SbiRet::no_shmem` | 当前 hart 的 NACL 共享内存不可用 |

#### 同步共享内存并模拟 `SRET` 指令

```rust
fn sync_sret(&self) -> SbiRet;
```

同步共享内存中的状态，恢复通用寄存器，并模拟 `SRET` 指令，将控制权转移到客户机的目标执行上下文。

调用方必须先在共享内存的嵌套 SRET 上下文中写入需要恢复的通用寄存器值。若 `SYNC_CSR` 功能可用，SBI 实现先同步全部已实现的 H 扩展 CSR；若 `SYNC_HFENCE` 功能可用，再同步全部 `HFENCE` 条目，随后恢复通用寄存器并模拟 `SRET`。

> 本函数仅在 `SYNC_SRET` 功能可用、且当前 hart 已设置 NACL 共享内存时生效。执行成功后，控制流按 `SRET` 的语义转移，本函数不应当返回。

本函数没有传入参数。

函数可能返回以下错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::not_supported` | `SYNC_SRET` 功能不可用 |
| `SbiRet::no_shmem` | 当前 hart 的 NACL 共享内存不可用 |

> 本函数不会返回 `SbiRet::success`。

### PMU 性能监测扩展

性能监测扩展允许系统软件查询、配置、启动和停止当前 hart 的性能计数器，用于统计处理器执行和 SBI 固件处理事件的情况。

硬件计数器由处理器提供，例如 `cycle`、`instret` 和 `hpmcounterX`。系统软件通过相应的只读 CSR 读取计数值，通过 SBI 调用请求更高特权级配置计数事件、启动或停止计数。固件计数器由 SBI 实现维护，可用于统计非对齐访问异常、核间中断和远程栅栏等事件，其计数值通过 SBI 接口读取。

> 硬件计数器的有效位宽由平台决定，最大为 64 位。固件计数器也可用 64 位数值表示。计数器的逻辑编号 `counter_idx` 不等于硬件 CSR 编号，应通过 `counter_get_info` 查询计数器的类型和硬件信息。

多个计数器使用 `counter_idx_base` 和 `counter_idx_mask` 共同选择：位掩码的第 `i` 位为 1 时，表示选择逻辑编号为 `counter_idx_base + i` 的计数器。

> `CounterMask` 可用于保存这两个数值，但本节 `Pmu` 特型的函数仍分别接收起始编号和位掩码。构造辅助结构时，`CounterMask::from_mask_base(mask, base)` 的参数顺序是位掩码在前；`into_inner()` 也返回 `(mask, base)`，与下列函数的参数顺序不同。

本扩展的特型描述如下。其中，`SIZE` 为 `sbi_spec::pmu::shmem_size::SIZE`，其值为 4096。

```rust
pub trait Pmu {
    // Required methods
    fn num_counters(&self) -> usize;
    fn counter_get_info(&self, counter_idx: usize) -> SbiRet;
    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet;
    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet;
    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet;
    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet;

    // Provided methods
    fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet { ... }
    fn snapshot_set_shmem(
        &self,
        shmem: SharedPtr<[u8; SIZE]>,
        flags: usize,
    ) -> SbiRet { ... }
}
```

PMU 扩展具有以下的函数。

#### 获取计数器数量

```rust
fn num_counters(&self) -> usize;
```

返回当前 hart 的硬件计数器和固件计数器总数。

本函数直接返回 `usize`。RustSBI 将它转换为 SBI 调用成功时的 `SbiRet::value`，对应 SBI 调用的 `error` 始终为成功。

本函数没有传入参数。

函数返回以下内容：

| 返回值 | 说明 |
|:----|:-----|
| `usize` | 硬件计数器和固件计数器的总数 |

#### 获取计数器信息

```rust
fn counter_get_info(&self, counter_idx: usize) -> SbiRet;
```

查询计数器类型，以及硬件计数器对应的 CSR 编号和有效位宽。成功时，`SbiRet::value` 包含以下字段。

| 位段 | 说明 |
|:----|:-----|
| 11:0 | 硬件计数器对应的 12 位 CSR 编号 |
| 17:12 | 硬件计数器的有效位数减 1。例如，63 表示 64 位计数器 |
| XLEN-2:18 | 保留 |
| XLEN-1 | 计数器类型：0 为硬件计数器，1 为固件计数器 |

> 若返回的类型为固件计数器，应忽略 CSR 编号和位宽字段。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `counter_idx` | 需要查询的计数器逻辑编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 查询成功，返回编码后的计数器信息 |
| `SbiRet::invalid_param` | `counter_idx` 不是有效的计数器编号 |

#### 匹配并配置计数器

```rust
fn counter_config_matching(
    &self,
    counter_idx_base: usize,
    counter_idx_mask: usize,
    config_flags: usize,
    event_idx: usize,
    event_data: u64,
) -> SbiRet;
```

在指定的计数器集合中，选择一个尚未启动、能够统计目标事件的计数器，并完成事件配置。成功时，`SbiRet::value` 为选中计数器的逻辑编号。

`event_idx` 是一个 20 位事件编号，其中第 19:16 位表示事件类型，第 15:0 位表示该类型中的事件代码。`event_data` 提供事件所需的额外配置，例如硬件原始事件的编码。

> 普通硬件事件、缓存事件和固件事件的类型及代码可使用 `sbi_spec::pmu` 中的常量描述。事件是否可用取决于平台和 SBI 实现；传入已定义的事件编号不保证平台一定具有对应的计数能力。

`config_flags` 的定义如下，可使用 `sbi_spec::pmu::flags::ConfigFlags` 组合标志，再通过 `.bits()` 取得参数值。

| 标志 | 位 | 说明 |
|:----|:---|:-----|
| `ConfigFlags::SKIP_MATCH` | 0 | 跳过匹配，直接选择集合中的第一个计数器 |
| `ConfigFlags::CLEAR_VALUE` | 1 | 配置时将计数值清零 |
| `ConfigFlags::AUTO_START` | 2 | 配置完成后自动启动计数器 |
| `ConfigFlags::SET_VUINH` | 3 | 请求禁止统计 VU 态事件 |
| `ConfigFlags::SET_VSINH` | 4 | 请求禁止统计 VS 态事件 |
| `ConfigFlags::SET_UINH` | 5 | 请求禁止统计 U 态事件 |
| `ConfigFlags::SET_SINH` | 6 | 请求禁止统计 S 态事件 |
| `ConfigFlags::SET_MINH` | 7 | 请求禁止统计 M 态事件 |
| 保留 | XLEN-1:8 | 应当为 0 |

> `AUTO_START` 本身不会清零或改变计数值。第 3 至 7 位是事件过滤提示；受平台能力或安全策略限制，SBI 实现可以忽略或覆盖这些提示。

函数传入 5 个参数：

| 参数 | 说明 |
|:----|:-----|
| `counter_idx_base` | 候选计数器集合的起始逻辑编号 |
| `counter_idx_mask` | 相对于起始编号的候选计数器位掩码 |
| `config_flags` | 配置和事件过滤标志 |
| `event_idx` | 需要统计的事件编号 |
| `event_data` | 事件的 64 位附加配置；具体含义由事件类型规定 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 配置成功，返回选中计数器的逻辑编号 |
| `SbiRet::invalid_param` | 所选集合至少包含一个无效的计数器 |
| `SbiRet::not_supported` | 所选集合中没有能够统计指定事件的计数器 |

#### 启动计数器

```rust
fn counter_start(
    &self,
    counter_idx_base: usize,
    counter_idx_mask: usize,
    start_flags: usize,
    initial_value: u64,
) -> SbiRet;
```

启动当前 hart 上指定的计数器，开始统计已经配置的事件。

`start_flags` 的定义如下，可使用 `sbi_spec::pmu::flags::StartFlags` 组合标志，再通过 `.bits()` 取得参数值。

| 标志 | 位 | 说明 |
|:----|:---|:-----|
| `StartFlags::INIT_VALUE` | 0 | 使用 `initial_value` 设置初始计数值 |
| `StartFlags::INIT_SNAPSHOT` | 1 | 从已配置的快照共享内存读取各计数器的初始值 |
| 保留 | XLEN-1:2 | 应当为 0 |

未设置初始化标志时，计数器从已有数值继续计数，`initial_value` 不生效。设置 `INIT_SNAPSHOT` 前，必须先为当前 hart 配置快照共享内存。

> `INIT_VALUE` 与 `INIT_SNAPSHOT` 互斥。使用 `INIT_VALUE` 时只选择一个计数器；需要为多个计数器分别设置初始值时，可使用共享快照。

本函数通过 `SbiRet::error` 报告结果，不通过 `value` 返回计数值。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `counter_idx_base` | 计数器集合的起始逻辑编号 |
| `counter_idx_mask` | 相对于起始编号的计数器位掩码 |
| `start_flags` | 启动和初始化标志 |
| `initial_value` | 设置 `INIT_VALUE` 时使用的 64 位初始值 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 计数器启动成功 |
| `SbiRet::invalid_param` | 所选集合至少包含一个无效的计数器 |
| `SbiRet::already_started` | 所选集合至少包含一个已经启动的计数器 |
| `SbiRet::no_shmem` | 设置了 `INIT_SNAPSHOT`，但没有可用的快照共享内存 |

#### 停止计数器

```rust
fn counter_stop(
    &self,
    counter_idx_base: usize,
    counter_idx_mask: usize,
    stop_flags: usize,
) -> SbiRet;
```

停止当前 hart 上指定的计数器，并根据标志解除事件配置或保存计数快照。

`stop_flags` 的定义如下，可使用 `sbi_spec::pmu::flags::StopFlags` 组合标志，再通过 `.bits()` 取得参数值。

| 标志 | 位 | 说明 |
|:----|:---|:-----|
| `StopFlags::RESET` | 0 | 重置计数器与事件的映射关系 |
| `StopFlags::TAKE_SNAPSHOT` | 1 | 将计数器的当前值保存到已配置的快照共享内存 |
| 保留 | XLEN-1:2 | 应当为 0 |

> `RESET` 重置的是事件映射关系，不表示将计数值清零。仅停止计数器而保留事件配置时，可不设置此标志。

设置 `TAKE_SNAPSHOT` 前，必须先为当前 hart 配置快照共享内存。保存快照时，实现应写入本次停止的计数器值并更新溢出位图，其它计数器的快照值保持不变。

本函数通过 `SbiRet::error` 报告结果，不通过 `value` 返回计数值。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `counter_idx_base` | 计数器集合的起始逻辑编号 |
| `counter_idx_mask` | 相对于起始编号的计数器位掩码 |
| `stop_flags` | 停止、重置事件映射和保存快照的标志 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 计数器停止成功 |
| `SbiRet::invalid_param` | 所选集合至少包含一个无效的计数器 |
| `SbiRet::already_stopped` | 所选集合至少包含一个已经停止的计数器 |
| `SbiRet::no_shmem` | 设置了 `TAKE_SNAPSHOT`，但没有可用的快照共享内存 |

#### 读取固件计数器

```rust
fn counter_fw_read(&self, counter_idx: usize) -> SbiRet;
```

读取固件计数器的当前值，成功时通过 `SbiRet::value` 返回。

若调用方 S 态的 XLEN 为 32，则仅返回计数值的低 32 位；在 RV64 平台下，可通过本函数取得完整的 64 位数值。

> 本函数只读取固件计数器。硬件计数器应通过相应的 CSR 读取，或在停止时使用共享快照取得计数值。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `counter_idx` | 需要读取的固件计数器逻辑编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回计数值；在 RV32 平台下为低 32 位 |
| `SbiRet::invalid_param` | `counter_idx` 指向硬件计数器，或不是有效的计数器编号 |

#### 读取固件计数器的高 32 位

```rust
fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet { ... }
```

读取固件计数器当前值的高 32 位。

若调用方 S 态的 XLEN 为 64 或更大，则成功返回的 `SbiRet::value` 为 0。RV32 平台可将本函数与 `counter_fw_read` 配合使用，以取得完整的计数值。

> 正在运行的计数器可能在两次读取之间变化。需要在 RV32 上拼接一致的 64 位结果时，可先读高位、再读低位、然后重新读取高位；两次高位不同则重试。也可先停止计数器，或使用共享快照。

> RustSBI 提供的默认方法在非 32 位目标上返回 `SbiRet::success(0)`，在 32 位目标上返回 `SbiRet::not_supported`。实现 RV32 的高位读取功能时，需要覆盖此方法。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `counter_idx` | 需要读取的固件计数器逻辑编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 读取成功，返回高 32 位；调用方 XLEN 为 64 或更大时为 0 |
| `SbiRet::invalid_param` | `counter_idx` 指向硬件计数器，或不是有效的计数器编号 |
| `SbiRet::not_supported` | 32 位目标使用了未提供高位读取能力的默认方法 |

#### 设置计数器快照共享内存

```rust
fn snapshot_set_shmem(
    &self,
    shmem: SharedPtr<[u8; SIZE]>,
    flags: usize,
) -> SbiRet { ... }
```

为当前 hart 设置并启用 PMU 快照共享内存，供 `counter_stop` 保存快照、`counter_start` 恢复计数值。

共享内存的物理基地址必须按 4096 字节对齐，区域大小必须为 4096 字节；此要求在 RV32 和 RV64 平台上相同。共享内存中的数据采用小端字节序，并应满足 SBI 的共享内存访问权限和内存属性要求。

`SharedPtr` 保存物理地址的低、高两个 XLEN 部分。传入 `SharedPtr::new(usize::MAX, usize::MAX)` 时，表示清除快照共享内存的配置并禁用快照功能，不表示传入一个实际可访问的物理地址。

> 应在启动期间为每个 hart 分别配置共享区。配置后，仅当 `counter_stop` 设置了 `TAKE_SNAPSHOT` 时，SBI 实现可以读写该共享区；仅当 `counter_start` 设置了 `INIT_SNAPSHOT` 时，可以读取该共享区。SBI 实现不得在其它时机访问该区域。

共享内存的布局如下。

| 字段 | 偏移 | 字节数 | 说明 |
|:----|:-----|:------|:-----|
| `counter_overflow_bitmap` | `0x0000` | 8 | 计数器溢出位图；仅在具有 `Sscofpmf` 扩展时有效，否则必须为 0 |
| `counter_values` | `0x0008` | 512 | 64 个 64 位计数值，可保存硬件计数器或固件计数器的快照 |
| 保留区域 | `0x0208` | 3576 | 保留供未来使用 |

> 位图中的第 `i` 位和数组的第 `i` 项，均对应本次启动或停止操作中编号为 `counter_idx_base + i` 的计数器，并非直接对应逻辑编号 `i`。更换起始编号后，同一共享区可用于另一组计数器。

本函数为可选功能，RustSBI 的默认实现返回 `SbiRet::not_supported`。`flags` 参数为保留参数，必须为 0。本函数通过 `SbiRet::error` 报告设置或禁用是否成功。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `shmem` | 4096 字节共享区的物理基地址；高、低地址部分均为 `usize::MAX` 时禁用快照 |
| `flags` | 保留参数，必须为 0 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 设置或清除快照共享内存配置成功 |
| `SbiRet::not_supported` | SBI 实现不支持 PMU 快照功能 |
| `SbiRet::invalid_param` | `flags` 不为 0，或启用快照时的基地址未按 4096 字节对齐 |
| `SbiRet::invalid_address` | 共享区不可写，或不满足 SBI 的其它共享内存要求 |

### SRST（Reset）系统重置扩展

系统重置扩展允许特权态（S 态）系统软件请求关机、冷重启或热重启。这里的“系统”是调用方所见的系统；对于虚拟机，重置操作由虚拟机监控程序实现，不一定改变物理设备的供电状态。

本扩展的特型描述如下。

```rust
pub trait Reset {
    // Required method
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet;
}
```

SRST 扩展具有以下的函数。

#### 重置系统

```rust
fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet;
```

按照指定的重置类型和原因重置系统。这是同步操作，成功后不返回；返回即表示重置失败。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `reset_type` | 重置类型，使用 `sbi_spec::srst` 中的 `RESET_TYPE_*` 常量，或平台定义的类型编号 |
| `reset_reason` | 重置原因，使用 `sbi_spec::srst` 中的 `RESET_REASON_*` 常量，或实现、平台定义的原因编号 |

标准重置类型如下。

| 常量 | 值 | 说明 |
|:----|:---|:-----|
| `RESET_TYPE_SHUTDOWN` | 0 | 关闭系统 |
| `RESET_TYPE_COLD_REBOOT` | 1 | 冷重启，原生执行环境中对应整个系统重新上电 |
| `RESET_TYPE_WARM_REBOOT` | 2 | 热重启，原生执行环境中对应主处理器及部分系统组件重新上电 |

标准重置原因包括 `RESET_REASON_NO_REASON`（0，无指定原因）和 `RESET_REASON_SYSTEM_FAILURE`（1，系统故障）。调用方应避免使用规范保留的编号。

函数可能返回以下错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::invalid_param` | `reset_type` 或 `reset_reason` 使用了保留值，或使用了未实现的平台自定义值 |
| `SbiRet::not_supported` | 重置类型已实现，但平台缺少执行该操作所需的依赖条件 |
| `SbiRet::failed` | 重置过程中发生了未指定的错误 |

> 重置系统函数不会返回 `SbiRet::success`，调用方也不应使用错误返回时的 `SbiRet::value`。

### STA 窃取时间统计扩展

窃取时间统计扩展允许 SBI 实现向特权态（S 态）系统软件报告虚拟 hart 的窃取时间和抢占状态。窃取时间是指虚拟 hart 已具备运行条件，却因物理处理器被其它任务占用等原因而未能运行的时间；虚拟 hart 自身处于空闲状态的时间不计入其中。

本扩展的特型描述如下。

```rust
pub trait Sta {
    // Required method
    fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet;
}
```

STA 扩展具有以下的函数。

#### 设置窃取时间共享内存

```rust
fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet;
```

为当前虚拟 hart 设置共享内存，并启用窃取时间信息报告。`SharedPtr<[u8; 64]>` 封装共享内存物理基地址的高、低部分，描述一个 64 字节的统计区域。

共享内存必须按 64 字节对齐，并提供至少 64 字节可写空间。SBI 实现必须在成功返回前将该统计区域清零。若物理地址的高、低部分均为 `usize::MAX`，则停止当前虚拟 hart 的信息报告，此时不访问共享内存。`flags` 为保留参数，必须为 0。

> 禁用报告时，可传入 `SharedPtr::new(usize::MAX, usize::MAX)`。普通共享内存地址的对齐要求不适用于这一禁用标记。

共享内存包含 `sequence`、`flags`、`steal` 和 `preempted` 等字段。`steal` 记录以纳秒为单位的累计窃取时间；读取前后应检查 `sequence`，只有两次读数相同且为偶数时，读取结果才有效。`preempted` 用于提示该虚拟 hart 是否被抢占。

> 统计区域使用期间，S 态软件不应改写其内容。系统重置或挂起等导致 S 态软件不可运行时，SBI 实现必须停止写入该区域。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `shmem` | 当前虚拟 hart 的 64 字节统计区域的共享物理地址，或高、低部分均为 `usize::MAX` 的禁用标记 |
| `flags` | 保留参数，必须为 0 |

本函数的 `SbiRet::value` 为 0，`SbiRet::error` 表示操作结果。

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 共享内存设置成功，或信息报告已禁用 |
| `SbiRet::invalid_param` | `flags` 不为 0，或共享内存基地址未按 64 字节对齐 |
| `SbiRet::invalid_address` | 共享内存不可写，或不满足 SBI 规范的共享内存地址范围要求 |
| `SbiRet::failed` | 设置过程中发生了未指定的错误 |

### SUSP 系统挂起扩展

系统挂起扩展允许特权态（S 态）系统软件请求进入系统级休眠状态。它针对调用方所见的系统，与 HSM 扩展中仅挂起单个 hart 的操作不同。

本扩展的特型描述如下。

```rust
pub trait Susp {
    // Required method
    fn system_suspend(
        &self,
        sleep_type: u32,
        resume_addr: usize,
        opaque: usize,
    ) -> SbiRet;
}
```

SUSP 扩展具有以下的函数。

#### 挂起系统

```rust
fn system_suspend(
    &self,
    sleep_type: u32,
    resume_addr: usize,
    opaque: usize,
) -> SbiRet;
```

请求系统进入 `sleep_type` 指定的休眠状态。挂起成功后，函数不返回；唤醒时，发起挂起的 hart 在 S 态从 `resume_addr` 指定的物理地址恢复执行。

休眠类型定义如下。

| `sleep_type` | 说明 |
|:-------------|:-----|
| 0 | `SUSPEND_TO_RAM`，挂起到内存 |
| `0x00000001..=0x7FFFFFFF` | 保留值 |
| `0x80000000..=0xFFFFFFFF` | 平台自定义休眠类型 |

实现 SUSP 扩展即表示支持挂起到内存。进入该状态前，除调用 hart 外的其它 hart 必须处于 HSM 的 `STOPPED` 状态，所有 hart 的寄存器和 CSR 状态必须保存到内存。

> 调用方还应按平台要求配置电源单元、电源域和唤醒设备。平台自定义的休眠类型及其唤醒设备由硬件描述说明，本扩展不提供休眠类型探测函数。

唤醒后，发起挂起的 hart 的初始寄存器内容如下。

| 寄存器 | 值 |
|:------|:----|
| `satp` | 0 |
| `sstatus.SIE` | 0 |
| `a0` | 当前 hart 的编号 |
| `a1` | `opaque` 参数内容 |

> 除以上寄存器外，其它寄存器的状态未定义。恢复代码应自行重建所需的执行环境。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `sleep_type` | 请求进入的系统休眠类型 |
| `resume_addr` | 唤醒后在 S 态开始执行的物理地址，必须能由 XLEN 位表示，并具有执行权限 |
| `opaque` | 唤醒后传入 `a1` 寄存器的值 |

函数可能返回以下错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::invalid_param` | `sleep_type` 为保留值，或对应的平台自定义类型未实现 |
| `SbiRet::not_supported` | 休眠类型已实现，但平台缺少进入该状态所需的依赖条件 |
| `SbiRet::invalid_address` | `resume_addr` 不是有效的物理地址，或 PMP 等保护机制、H 扩展的 G 阶段地址转换禁止执行该地址的指令 |
| `SbiRet::denied` | 未满足进入所选休眠状态的条件 |
| `SbiRet::failed` | 挂起过程中发生了未指定的错误 |

> 唤醒后的执行入口是 `resume_addr`，不会通过原调用点返回 `SbiRet::success`。

### TIME（Timer）定时器扩展

定时器扩展允许特权态（S 态）系统软件设置当前 hart 的下一次定时器事件。

本扩展的特型描述如下。

```rust
pub trait Timer {
    // Required method
    fn set_timer(&self, stime_value: u64);
}
```

TIME 扩展具有以下的函数。

#### 设置定时器

```rust
fn set_timer(&self, stime_value: u64);
```

将下一次定时器事件设置到 `stime_value` 指定的绝对时间。当目标时间在未来时，实现必须清除待处理的定时器中断，与该中断是否被屏蔽无关。

> 若要清除待处理的定时器中断，并将下一次事件推迟到极远的将来，可将 `stime_value` 设置为 `u64::MAX`。若只需屏蔽定时器中断，则可清除 `sie.STIE` 位。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `stime_value` | 下一次定时器事件的 64 位绝对时间，使用平台定时器的时间基准，不是相对延时 |

本函数不返回 `SbiRet`。RustSBI 在完成此方法调用后，向 SBI 调用方返回成功结果。

| 返回值 | 说明 |
|:----|:-----|
| `()` | 定时器设置完成；该特型方法不提供错误返回值 |

### SSE 软件事件扩展

软件事件扩展允许 SBI 实现向特权态（S 态）系统软件投递事件，用于处理可靠性、可用性和可维护性（RAS）事件、性能计数器溢出等情况。SSE 事件可以抢占普通异常和中断，也允许高优先级事件抢占正在处理的低优先级事件。

> SSE 是 SBI 3.0 引入的扩展。是否支持某个事件，还取决于平台硬件和 SBI 实现；扩展可用不代表所有事件都可用。

事件分为本地事件和全局事件。本地事件的状态由每个 hart 分别维护，需要接收事件的 hart 应分别注册和启用它。全局事件由所有 hart 共享状态，只需注册和启用一次，SBI 实现选择一个能够接收软件事件的 hart 执行处理函数。

每个事件具有 32 位的 `event_id`，可使用 `sbi_spec::sse::event_id` 中的常量指定。事件在使用过程中具有以下状态。

| 状态 | 值 | 说明 |
|:----|:---|:-----|
| `UNUSED` | 0 | 尚未注册处理函数 |
| `REGISTERED` | 1 | 已注册处理函数，尚未启用事件 |
| `ENABLED` | 2 | 事件已启用，可以在 hart 允许接收时投递 |
| `RUNNING` | 3 | 正在处理事件 |

> 事件优先级是 32 位无符号整数，数值越小，优先级越高。优先级相同时，`event_id` 较小的事件优先。事件的初始优先级为 0。

事件启用状态和 hart 的屏蔽状态共同决定是否可以投递事件。系统软件既需要注册并启用事件，也需要通过 `hart_unmask` 允许对应 hart 接收软件事件。

本扩展的特型描述如下。

```rust
pub trait Sse {
    // Required methods
    fn read_attrs(
        &self,
        event_id: u32,
        base_attr_id: u32,
        attr_count: u32,
        output: SharedPtr<u8>,
    ) -> SbiRet;
    fn write_attrs(
        &self,
        event_id: u32,
        base_attr_id: u32,
        attr_count: u32,
        input: SharedPtr<u8>,
    ) -> SbiRet;
    fn register(
        &self,
        event_id: u32,
        handler_entry_pc: usize,
        handler_entry_arg: usize,
    ) -> SbiRet;
    fn unregister(&self, event_id: u32) -> SbiRet;
    fn enable(&self, event_id: u32) -> SbiRet;
    fn disable(&self, event_id: u32) -> SbiRet;
    fn complete(&self) -> SbiRet;
    fn inject(&self, event_id: u32, hart_id: usize) -> SbiRet;
    fn hart_unmask(&self) -> SbiRet;
    fn hart_mask(&self) -> SbiRet;
}
```

SSE 扩展具有以下的函数。

#### 读取事件属性

```rust
fn read_attrs(
    &self,
    event_id: u32,
    base_attr_id: u32,
    attr_count: u32,
    output: SharedPtr<u8>,
) -> SbiRet;
```

读取指定事件的一组属性，将属性值写入输出共享内存。

属性编号从 `base_attr_id` 开始，连续读取 `attr_count` 个属性；`attr_count` 必须大于 0。每个属性值占 XLEN 位，而属性编号本身为 32 位。

`output` 表示共享内存的物理基地址。该地址必须按 `XLEN / 8` 字节对齐，输出区域大小按 `(XLEN / 8) * attr_count` 字节计算。共享内存采用小端字节序，且系统软件必须具有对此区域的写入权限。

> `SharedPtr<u8>` 保存物理地址的高、低部分，不自带区域长度。这里的 `u8` 不表示只读写一个字节，实际传输大小由调用方 XLEN 和 `attr_count` 决定。

常用属性如下，可使用 `sbi_spec::sse::attr_id` 中的同名常量指定。

| 属性 | 编号 | 访问权限 | 说明 |
|:----|:-----|:--------|:-----|
| `STATUS` | 0 | 只读 | 第 1:0 位为事件状态，第 2 位为待处理标志，第 3 位表示是否允许软件注入 |
| `PRIORITY` | 1 | 读写 | 事件优先级，仅低 32 位有效，其余位为 0 |
| `CONFIG` | 2 | 读写 | 第 0 位为一次性事件标志，置 1 后在完成处理时自动禁用事件 |
| `PREFERRED_HART` | 3 | 全局事件可读写；本地事件只读 | 优先选择的 hart 编号 |
| `ENTRY_PC` | 4 | 只读 | 已注册处理函数的入口程序计数器值 |
| `ENTRY_ARG` | 5 | 只读 | 进入处理函数时通过 `a7` 传递的参数 |
| `INTERRUPTED_SEPC` | 6 | 读写 | 投递事件前保存的 `sepc` 寄存器值 |
| `INTERRUPTED_FLAGS` | 7 | 读写 | 投递事件前保存的特权级、中断和虚拟化等状态位 |
| `INTERRUPTED_A6` | 8 | 读写 | 投递事件前保存的 `a6` 寄存器值 |
| `INTERRUPTED_A7` | 9 | 读写 | 投递事件前保存的 `a7` 寄存器值 |

> `STATUS` 的待处理标志由事件源设置，事件进入 `RUNNING` 状态时清除。全局事件的 `PREFERRED_HART` 是路由提示，SBI 实现可以选择另一个允许接收事件的 hart。

本函数通过 `SbiRet::error` 报告结果，属性值位于共享内存中。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `event_id` | 需要读取属性的事件编号 |
| `base_attr_id` | 起始属性编号 |
| `attr_count` | 连续读取的属性数量，必须大于 0 |
| `output` | 输出共享内存的物理基地址 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 属性读取成功 |
| `SbiRet::not_supported` | 事件编号有效且不是保留编号，但平台不支持该事件 |
| `SbiRet::invalid_param` | 事件编号无效，或 `attr_count` 为 0 |
| `SbiRet::bad_range` | 指定的属性编号范围包含保留属性 |
| `SbiRet::invalid_address` | 输出区域不满足共享内存要求，包括对齐或访问权限要求 |
| `SbiRet::failed` | 读取过程中发生了未指定的错误 |

#### 写入事件属性

```rust
fn write_attrs(
    &self,
    event_id: u32,
    base_attr_id: u32,
    attr_count: u32,
    input: SharedPtr<u8>,
) -> SbiRet;
```

从输入共享内存取得属性值，写入指定事件的一组连续属性。

属性编号从 `base_attr_id` 开始，连续写入 `attr_count` 个属性；`attr_count` 必须大于 0。`input` 必须按 `XLEN / 8` 字节对齐，输入区域大小按 `(XLEN / 8) * attr_count` 字节计算。属性值采用小端字节序，系统软件必须具有对此区域的读取权限。

本地事件的属性修改只影响调用 hart；全局事件的属性修改影响所有 hart 共享的事件状态。可写属性还受到以下状态限制。

| 属性 | 可以修改的条件 |
|:----|:--------------|
| `PRIORITY`、`CONFIG` | 事件处于 `UNUSED` 或 `REGISTERED` 状态 |
| `PREFERRED_HART` | 必须是全局事件，且处于 `UNUSED` 或 `REGISTERED` 状态；值必须为有效的 hart 编号 |
| `INTERRUPTED_SEPC`、`INTERRUPTED_FLAGS`、`INTERRUPTED_A6`、`INTERRUPTED_A7` | 事件处于 `RUNNING` 状态；对于全局事件，只有正在执行处理函数的 hart 可以修改 |

`CONFIG` 的第 0 位表示一次性事件，其余位必须为 0。`INTERRUPTED_FLAGS` 中的第 0 至 5 位分别保存 `sstatus.SPP`、`sstatus.SPIE`、`hstatus.SPV`、`hstatus.SPVP`、`sstatus.SPELP` 和 `sstatus.SDT`，其余位必须为 0；依赖可选指令集扩展的字段只在相应扩展可用时有意义。

> `ENTRY_PC` 和 `ENTRY_ARG` 通过 `register` 设置，不能通过本函数修改。若多个属性值存在错误，实现按属性编号顺序报告遇到的第一个错误。

本函数通过 `SbiRet::error` 报告结果。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `event_id` | 需要写入属性的事件编号 |
| `base_attr_id` | 起始属性编号 |
| `attr_count` | 连续写入的属性数量，必须大于 0 |
| `input` | 输入共享内存的物理基地址 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 属性写入成功 |
| `SbiRet::not_supported` | 事件编号有效且不是保留编号，但平台不支持该事件 |
| `SbiRet::invalid_param` | 事件编号无效、`attr_count` 为 0，或属性值不合法 |
| `SbiRet::denied` | 所选范围中存在只读属性 |
| `SbiRet::invalid_state` | 事件状态或调用 hart 不满足属性的修改条件 |
| `SbiRet::bad_range` | 指定的属性编号范围包含保留属性 |
| `SbiRet::invalid_address` | 输入区域不满足共享内存要求，包括对齐或访问权限要求 |
| `SbiRet::failed` | 写入过程中发生了未指定的错误 |

#### 注册事件处理函数

```rust
fn register(
    &self,
    event_id: u32,
    handler_entry_pc: usize,
    handler_entry_arg: usize,
) -> SbiRet;
```

为事件注册 S 态处理函数及其入口参数。事件必须处于 `UNUSED` 状态；注册成功后进入 `REGISTERED` 状态，之后还需调用 `enable` 才能启用事件。

本地事件仅为调用 hart 注册处理函数，全局事件的注册对所有 hart 生效。

`handler_entry_pc` 是事件处理函数的入口程序计数器值，必须按 2 字节对齐。事件投递时不会像 `hart_start` 那样将 `satp` 清零，入口按届时的 S 态地址转换环境解释。系统软件应保证可能接收该事件的 hart 都能在相应环境中执行此入口。

> 此入口地址与 `read_attrs`、`write_attrs` 使用的共享内存物理地址含义不同。启用分页时，处理函数入口需要具有可执行的虚拟地址映射。

进入事件处理函数时，主要寄存器和状态如下。

| 寄存器或状态 | 内容 |
|:------------|:-----|
| 程序计数器 | `handler_entry_pc` |
| `a6` | 当前 hart 编号 |
| `a7` | `handler_entry_arg` |
| `sepc` | 被事件中断的程序计数器值 |
| `sstatus.SPP` | 被中断程序的特权级 |
| `sstatus.SPIE` | 被中断时的 `sstatus.SIE` |
| `sstatus.SIE` | 0 |
| 特权级和虚拟化状态 | S 态，虚拟化关闭 |

> 原来的 `a6`、`a7`、`sepc` 和有关状态位由 SBI 实现保存到事件属性中。处理函数入口不会获得由 SSE 自动设置的新栈；需要使用专门的入口代码保存其它被修改的寄存器，并在调用 `complete` 前恢复它们。

> 不同事件可以使用不同的 `handler_entry_arg`，以便处理函数区分事件并管理嵌套上下文。此参数通过 `a7` 传递，不遵循普通 Rust 函数将第一个参数放在 `a0` 的调用约定。

若 S 态可使用 H 扩展，进入处理函数时还会保存并更新有关虚拟化状态；可使用 `Zicfilp` 或 `Ssdbltrp` 时，也需要按 SSE 规则保存和设置对应状态。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `event_id` | 需要注册处理函数的事件编号 |
| `handler_entry_pc` | 2 字节对齐的处理函数入口程序计数器值 |
| `handler_entry_arg` | 进入处理函数时通过 `a7` 传递的参数 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 注册成功，事件进入 `REGISTERED` 状态 |
| `SbiRet::not_supported` | 事件编号有效且不是保留编号，但平台不支持该事件 |
| `SbiRet::invalid_state` | 事件不处于 `UNUSED` 状态 |
| `SbiRet::invalid_param` | 事件编号无效，或入口未按 2 字节对齐 |

#### 注销事件处理函数

```rust
fn unregister(&self, event_id: u32) -> SbiRet;
```

注销指定事件的处理函数，使事件从 `REGISTERED` 状态进入 `UNUSED` 状态。

本地事件仅在调用 hart 上注销，全局事件的注销对所有 hart 生效。已经启用的事件应先通过 `disable` 回到 `REGISTERED` 状态。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `event_id` | 需要注销处理函数的事件编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 注销成功，事件进入 `UNUSED` 状态 |
| `SbiRet::not_supported` | 事件编号有效且不是保留编号，但平台不支持该事件 |
| `SbiRet::invalid_state` | 事件不处于 `REGISTERED` 状态 |
| `SbiRet::invalid_param` | 事件编号无效 |

#### 启用事件

```rust
fn enable(&self, event_id: u32) -> SbiRet;
```

启用已经注册的事件，使事件从 `REGISTERED` 状态进入 `ENABLED` 状态。

本地事件仅在调用 hart 上启用，全局事件的启用对所有 hart 生效。事件启用后，还需要目标 hart 已解除 SSE 屏蔽，才能投递到该 hart。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `event_id` | 需要启用的事件编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 启用成功，事件进入 `ENABLED` 状态 |
| `SbiRet::not_supported` | 事件编号有效且不是保留编号，但平台不支持该事件 |
| `SbiRet::invalid_param` | 事件编号无效 |
| `SbiRet::invalid_state` | 事件不处于 `REGISTERED` 状态 |

#### 禁用事件

```rust
fn disable(&self, event_id: u32) -> SbiRet;
```

禁用已经启用的事件，使事件从 `ENABLED` 状态回到 `REGISTERED` 状态，保留已注册的处理函数。

本地事件仅在调用 hart 上禁用，全局事件的禁用对所有 hart 生效。

> 本函数要求事件处于 `ENABLED` 状态，不能直接禁用 `RUNNING` 状态的事件。需要处理一次后自动禁用的事件，可在启用前设置 `CONFIG` 的一次性事件标志，随后通过 `complete` 完成处理。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `event_id` | 需要禁用的事件编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 禁用成功，事件回到 `REGISTERED` 状态 |
| `SbiRet::not_supported` | 事件编号有效且不是保留编号，但平台不支持该事件 |
| `SbiRet::invalid_param` | 事件编号无效 |
| `SbiRet::invalid_state` | 事件不处于 `ENABLED` 状态 |

#### 完成事件处理

```rust
fn complete(&self) -> SbiRet;
```

完成当前 hart 上优先级最高、处于 `RUNNING` 状态的事件处理。

若当前 hart 没有正在处理的事件，本函数不改变事件状态，正常返回成功。若存在正在处理的事件，则完成该事件并恢复被中断的执行上下文；对于系统软件调用方，控制流转到恢复的位置，不继续执行事件处理函数中这次 SBI 调用之后的指令。

一次性事件完成后回到 `REGISTERED` 状态，其它事件回到 `ENABLED` 状态。若当前事件曾抢占另一个事件，完成后恢复较低优先级事件的处理。

恢复过程需要区分“返回到哪里”与“恢复被覆盖的寄存器值”。进入事件处理函数时，原来的 `sepc` 保存在 `INTERRUPTED_SEPC` 属性中，而处理函数看到的 `sepc` 已记录被中断的程序计数器。完成事件时，先使用当前的返回状态确定恢复位置和执行模式，再恢复被 SSE 覆盖的寄存器。

| 恢复目标 | 使用的值 |
|:--------|:---------|
| 程序计数器 | 完成处理时的 `sepc` |
| 执行特权级 | 完成处理时的 `sstatus.SPP` |
| 虚拟化状态（支持 H 扩展时） | 完成处理时的 `hstatus.SPV` |
| `sstatus.SIE` | 完成处理时的 `sstatus.SPIE` |
| `sstatus.SPP`、`sstatus.SPIE` | `INTERRUPTED_FLAGS` 的第 0、1 位 |
| `hstatus.SPV`、`hstatus.SPVP`（支持 H 扩展时） | `INTERRUPTED_FLAGS` 的第 2、3 位 |
| `sstatus.SPELP`（支持 `Zicfilp` 时） | `INTERRUPTED_FLAGS` 的第 4 位 |
| `sstatus.SDT`（支持 `Ssdbltrp` 时） | `INTERRUPTED_FLAGS` 的第 5 位 |
| `a6`、`a7` | `INTERRUPTED_A6`、`INTERRUPTED_A7` 属性 |
| `sepc` | `INTERRUPTED_SEPC` 属性 |

> 表中的“完成处理时”指恢复操作覆盖寄存器之前的值。例如，不能先用属性覆盖 `sepc`，再把覆盖后的值作为本次恢复的程序计数器。

> 处理函数负责恢复自己修改的其它通用寄存器。SBI 实现还应确保完成事件的异常返回路径使用恢复后的上下文，不能按普通 SBI 返回路径覆盖被中断程序的 `a0`、`a1` 等寄存器。

本函数没有传入参数。

函数可能产生以下结果：

| 返回值或控制流 | 说明 |
|:--------------|:-----|
| `SbiRet::success` | 当前没有 `RUNNING` 事件，函数正常返回 |
| 恢复被中断上下文 | 完成优先级最高的 `RUNNING` 事件，并转移到恢复的程序位置 |

#### 注入软件事件

```rust
fn inject(&self, event_id: u32, hart_id: usize) -> SbiRet;
```

请求注入指定的软件事件。事件的 `STATUS` 属性第 3 位必须表明允许通过软件注入。

对于本地事件，`hart_id` 指定目标 hart；对于全局事件，忽略 `hart_id`，由事件路由规则选择处理 hart。

若在事件处理函数中注入另一个已经具备投递条件的事件，则按事件优先级决定处理时机：高优先级事件可以立即抢占当前事件，较低优先级事件等待当前事件完成。

> 本函数成功表示注入请求成功，不表示目标事件的处理函数已经执行完毕。事件仍受注册、启用和 hart 屏蔽状态约束。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `event_id` | 需要注入的事件编号 |
| `hart_id` | 本地事件的目标 hart 编号；全局事件忽略此参数 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 事件注入成功 |
| `SbiRet::not_supported` | 事件编号有效且不是保留编号，但平台不支持该事件 |
| `SbiRet::invalid_param` | 事件编号无效，或本地事件的目标 hart 编号无效 |
| `SbiRet::failed` | 注入过程中发生了未指定的错误 |

#### 允许当前 hart 接收软件事件

```rust
fn hart_unmask(&self) -> SbiRet;
```

解除当前 hart 对软件事件的屏蔽，使其能够接收已注册并启用的事件。

所有 hart 的软件事件初始状态均为屏蔽。系统软件应在启动期间完成事件处理环境的准备后，对需要接收软件事件的 hart 分别调用本函数。

> 此操作改变 hart 是否允许接收软件事件，不代替单个事件的 `register` 和 `enable` 操作。

本函数没有传入参数。

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 当前 hart 已解除软件事件屏蔽 |
| `SbiRet::already_started` | 当前 hart 原本已允许接收软件事件 |
| `SbiRet::failed` | 解除屏蔽过程中发生了未指定的错误 |

#### 屏蔽当前 hart 的软件事件

```rust
fn hart_mask(&self) -> SbiRet;
```

屏蔽当前 hart 的软件事件，使其不再接收新投递的软件事件。

此操作改变当前 hart 的接收状态，不注销处理函数，也不代替单个事件的 `disable` 操作。需要重新接收事件时，可调用 `hart_unmask`。

> SSE 事件具有独立的 hart 屏蔽操作。需要屏蔽软件事件时，应使用本函数，不能仅依靠关闭普通 S 态中断。

本函数没有传入参数。

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 当前 hart 已屏蔽软件事件 |
| `SbiRet::already_stopped` | 当前 hart 原本已屏蔽软件事件 |
| `SbiRet::failed` | 屏蔽过程中发生了未指定的错误 |

### FWFT 固件特性扩展

固件特性扩展允许特权态（S 态）系统软件查询和配置特定的硬件能力或 SBI 实现功能。每项特性由一个 32 位编号标识，并具有规定的取值范围和作用域。局部特性作用于当前 hart，全局特性作用于整个系统。

本扩展的特型描述如下。

```rust
pub trait Fwft {
    // Required methods
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet;
    fn get(&self, feature_id: u32) -> SbiRet;
}
```

标准特性编号由 `sbi_spec::fwft::feature_type` 模块提供。以下特性均为局部特性。

| 常量 | 值 | 说明 |
|:----|:---|:-----|
| `MISALIGNED_EXC_DELEG` | 0 | 控制是否将非对齐访问异常委托给 S 态；0 为禁用，1 为启用 |
| `LANDING_PAD` | 1 | 控制 S 态的着陆垫支持；0 为禁用，1 为启用 |
| `SHADOW_STACK` | 2 | 控制 S 态的影子栈支持；0 为禁用，1 为启用 |
| `DOUBLE_TRAP` | 3 | 控制双重陷阱支持；0 为禁用，1 为启用 |
| `PTE_AD_HW_UPDATING` | 4 | 控制硬件是否更新 S 态页表项的 A/D 位；0 为禁用，1 为启用 |
| `POINTER_MASKING_PMLEN` | 5 | 配置 S 态的指针屏蔽长度；0 为禁用，非零值指定受支持的 PMLEN |

> 这些常量的类型为 `usize`，传给 `feature_id` 时应转换为 `u32`。特性编号的定义不代表平台一定实现对应功能。

非保留型挂起期间，SBI 实现保存特性配置，并在恢复时还原。hart 重置使局部特性恢复默认值，系统重置使全局和局部特性均恢复默认值。

FWFT 扩展具有以下的函数。

#### 设置固件特性

```rust
fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet;
```

根据 `value` 和 `flags` 设置指定的固件特性。将特性设置为其当前值也属于成功操作；设置失败时，原有配置值保持不变。

`flags` 的第 0 位为 `LOCK`，表示设置后锁定该特性的值。对于局部特性，锁定持续到对应 hart 重置；对于全局特性，锁定持续到系统重置。其它位均为保留位，必须为 0。

> 可使用 `sbi_spec::fwft::flags::SetFlags::LOCK.bits()` 构造锁定标志。锁定会限制后续配置修改，固件实现应区分一般的访问拒绝与配置已锁定两种情况。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `feature_id` | 需要设置的固件特性编号 |
| `value` | 新的配置值，必须满足该特性的取值要求 |
| `flags` | 设置标志；0 表示普通设置，第 0 位为 1 表示设置后锁定 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 特性设置成功 |
| `SbiRet::not_supported` | 特性编号有效，但平台缺少所需的硬件或 SBI 实现支持 |
| `SbiRet::invalid_param` | `value` 或 `flags` 无效 |
| `SbiRet::denied` | SBI 实现拒绝设置，或特性编号为保留值、未实现的平台自定义值 |
| `SbiRet::denied_locked` | 特性已锁定，禁止修改 |
| `SbiRet::failed` | 设置过程中发生了未指定的错误 |

#### 获取固件特性

```rust
fn get(&self, feature_id: u32) -> SbiRet;
```

获取指定固件特性的配置值。成功时，`SbiRet::value` 为当前配置值；失败时，`SbiRet::value` 为 0。

> `get` 返回配置值，不返回设置时使用的 `LOCK` 标志。配置值 0 的含义由特性定义，不能仅凭 `value` 是否为 0 判断调用是否成功。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `feature_id` | 需要查询的固件特性编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 查询成功，返回特性的当前配置值 |
| `SbiRet::not_supported` | 特性编号有效，但平台缺少所需的硬件或 SBI 实现支持 |
| `SbiRet::denied` | 特性编号为保留值，或对应的平台自定义特性未实现 |
| `SbiRet::failed` | 查询过程中发生了未指定的错误 |

### DBTR 调试触发器扩展

调试触发器扩展允许特权态（S 态）系统软件通过 SBI 配置和管理当前 hart 的硬件调试触发器，也可用于在 HS 态虚拟机监控程序与 VS 态客户机之间共享触发器资源。

每个 hart 的触发器总数称为 `trig_max`。SBI 实现为触发器分配逻辑编号 `trig_idx`，其范围为 `0..trig_max`。逻辑编号与硬件触发器编号不必相同，不同 hart 的触发器数量也可能不同。

触发器配置使用 `trig_tdata1`、`trig_tdata2` 和 `trig_tdata3` 三个字，其编码分别对应 RISC-V Sdtrig 扩展的 `tdata1`、`tdata2` 和 `tdata3` CSR。其中，`trig_tdata1.dmode` 和 `trig_tdata1.m` 必须为 0。

本扩展的特型描述如下。

```rust
use rustsbi::spec::binary::TriggerMask;

pub trait Dbtr {
    // Required methods
    fn num_triggers(&self, trig_tdata1: usize) -> usize;
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet;
    fn read_triggers(&self, trig_idx_base: usize, trig_count: usize) -> SbiRet;
    fn install_triggers(&self, trig_count: usize) -> SbiRet;
    fn update_triggers(&self, trig_count: usize) -> SbiRet;
    fn uninstall_triggers(&self, triggers: TriggerMask) -> SbiRet;
    fn enable_triggers(&self, triggers: TriggerMask) -> SbiRet;
    fn disable_triggers(&self, triggers: TriggerMask) -> SbiRet;
}
```

> `TriggerMask` 封装触发器位掩码和逻辑编号基值，通过 `rustsbi::spec::binary::TriggerMask` 导入。使用 `TriggerMask::from_mask_base(mask, base)` 构造时，`mask` 的第 `i` 位选中逻辑编号为 `base + i` 的触发器。

> 卸载、启用和禁用触发器的标准 SBI 调用按 `(trig_idx_base, trig_idx_mask)` 传参，与 `TriggerMask` 构造函数的顺序相反。派生宏的 DBTR 分派路径尚未转换这一顺序，使用这些操作时需在调用分派层校正；S 态调用方仍应遵循标准 ABI。

读出、安装和更新触发器配置均使用 `set_shmem` 注册的共享内存。每条记录占 4 个 XLEN 位的字，按小端序存储，第 `i` 条记录的字节偏移为 `i * (XLEN / 2)`。记录布局如下。

| 字序号 | 读取时 | 安装时 | 更新时 |
|:------|:-------|:-------|:-------|
| 0 | 输出触发器状态 `trig_state` | 输出新分配的逻辑编号 `trig_idx` | 输入已有逻辑编号 `trig_idx` |
| 1 | 输出 `trig_tdata1` | 输入 `trig_tdata1` | 输入 `trig_tdata1` |
| 2 | 输出 `trig_tdata2` | 输入 `trig_tdata2` | 输入 `trig_tdata2` |
| 3 | 输出 `trig_tdata3` | 输入 `trig_tdata3` | 输入 `trig_tdata3` |

`trig_state` 保存逻辑触发器的映射状态和模式使能位。其 `mapped` 位表示是否已映射到硬件触发器，`have_hw_trig` 位表示硬件编号是否有效。

DBTR 扩展具有以下的函数。

#### 获取触发器数量

```rust
fn num_triggers(&self, trig_tdata1: usize) -> usize;
```

获取当前 hart 上能够支持指定配置的调试触发器数量。`trig_tdata1` 为 0 时，返回当前 hart 的触发器总数 `trig_max`；否则，返回与所给配置匹配的触发器数量。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `trig_tdata1` | 用于匹配触发器能力的配置字；0 表示查询总数 |

本函数直接返回 `usize`，不返回 `SbiRet`。RustSBI 将此数量作为成功结果的 `SbiRet::value` 交给 SBI 调用方。

| 返回值 | 说明 |
|:----|:-----|
| `usize` | 当前 hart 的触发器总数，或支持指定配置的触发器数量 |

#### 设置触发器共享内存

```rust
fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet;
```

为当前 hart 设置调试触发器配置的共享内存。`shmem` 封装共享内存物理基地址的高、低部分。

普通共享内存地址必须按 `XLEN / 8` 字节对齐，并提供 `trig_max * (XLEN / 2)` 字节空间，以容纳每个触发器的四字记录。例如，RV64 下按 8 字节对齐，每个触发器需要 32 字节；RV32 下按 4 字节对齐，每个触发器需要 16 字节。

> `SharedPtr<u8>` 仅携带地址，不记录缓冲区长度。调用方应根据 `num_triggers(0)` 的结果分配足够的空间，实现方应检查对应的物理地址范围。

若物理地址高、低部分均为 `usize::MAX`，则禁用触发器共享内存。可使用 `SharedPtr::new(usize::MAX, usize::MAX)` 表示这一特殊地址。`flags` 为保留参数，必须为 0。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `shmem` | 当前 hart 的调试触发器共享物理地址，或高、低部分均为 `usize::MAX` 的禁用标记 |
| `flags` | 保留参数，必须为 0 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 共享内存设置成功，或已禁用 |
| `SbiRet::invalid_param` | `flags` 不为 0，或普通共享内存地址未按 `XLEN / 8` 字节对齐 |
| `SbiRet::invalid_address` | 共享内存不满足 SBI 规范的物理地址范围要求 |
| `SbiRet::failed` | 设置过程中发生了未指定的错误 |

#### 读取触发器配置

```rust
fn read_triggers(&self, trig_idx_base: usize, trig_count: usize) -> SbiRet;
```

读取当前 hart 上从 `trig_idx_base` 开始的连续 `trig_count` 个逻辑触发器，将其状态和配置写入共享内存。第 `i` 条记录对应逻辑编号 `trig_idx_base + i`，其第一个字为 `trig_state`。

> 调用前必须通过 `set_shmem` 注册共享内存。读取记录中的 `trig_state` 与安装、更新记录中的 `trig_idx` 含义不同，不能直接互换。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `trig_idx_base` | 需要读取的第一个逻辑触发器编号 |
| `trig_count` | 需要读取的触发器数量 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 状态和配置读取成功，结果位于共享内存 |
| `SbiRet::no_shmem` | 当前 hart 的调试触发器共享内存未启用 |
| `SbiRet::bad_range` | `trig_idx_base >= trig_max`，或 `trig_idx_base + trig_count >= trig_max` |

#### 安装触发器

```rust
fn install_triggers(&self, trig_count: usize) -> SbiRet;
```

根据当前 hart 共享内存起始位置的 `trig_count` 条配置记录安装触发器。SBI 实现按记录顺序分配逻辑编号和匹配的硬件触发器，保存模式使能位，写入硬件配置，并将分配的 `trig_idx` 回填到各记录的第一个字。

触发器链必须分配连续的逻辑编号和连续的硬件触发器。若最后一条记录的 `trig_tdata1.type` 为 2 或 6，则其 `chain` 位必须为 0，以避免配置数组包含不完整的触发器链。

成功或共享内存未启用时，`SbiRet::value` 为 0；其它失败情况下，`SbiRet::value` 为失败配置在数组中的下标。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `trig_count` | 从共享内存偏移 0 开始的触发器配置记录数 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 触发器安装成功，分配的逻辑编号已回填 |
| `SbiRet::no_shmem` | 当前 hart 的调试触发器共享内存未启用 |
| `SbiRet::bad_range` | `trig_count >= trig_max` |
| `SbiRet::invalid_param` | 某条记录的 `trig_tdata1`、`trig_tdata2` 或 `trig_tdata3` 配置无效 |
| `SbiRet::failed` | 无法为某条配置分配逻辑编号或硬件触发器 |
| `SbiRet::not_supported` | 某条配置依赖硬件 `tdata1`、`tdata2` 或 `tdata3` CSR 中未实现的可选位 |

#### 更新触发器

```rust
fn update_triggers(&self, trig_count: usize) -> SbiRet;
```

根据当前 hart 共享内存起始位置的 `trig_count` 条配置记录更新已安装的触发器。每条记录的第一个字必须包含待更新触发器的逻辑编号 `trig_idx`。

更新按记录顺序进行。`trig_tdata1.type` 和 `trig_tdata1.chain` 必须与安装时的值一致；SBI 实现保存新的模式使能位，并更新相应硬件触发器的配置。

> 本函数修改已有触发器配置，不分配新的逻辑编号。改变触发器类型或链结构时，应先卸载相关触发器，再安装新的配置。

成功或共享内存未启用时，`SbiRet::value` 为 0；其它失败情况下，`SbiRet::value` 为失败配置在数组中的下标。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `trig_count` | 从共享内存偏移 0 开始的触发器配置记录数 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 触发器配置更新成功 |
| `SbiRet::no_shmem` | 当前 hart 的调试触发器共享内存未启用 |
| `SbiRet::bad_range` | `trig_count >= trig_max` |
| `SbiRet::invalid_param` | 某条记录的 `trig_idx` 越界，或 `trig_tdata1`、`trig_tdata2`、`trig_tdata3` 配置无效 |
| `SbiRet::failed` | 某条记录的逻辑编号有效，但尚未映射到硬件触发器 |
| `SbiRet::not_supported` | 某条配置依赖硬件 `tdata1`、`tdata2` 或 `tdata3` CSR 中未实现的可选位 |

#### 卸载触发器

```rust
fn uninstall_triggers(&self, triggers: TriggerMask) -> SbiRet;
```

卸载当前 hart 上由 `triggers` 选中的触发器。SBI 实现清除对应硬件触发器的配置及保存的 `trig_state`，解除映射，并释放逻辑编号和硬件触发器，以供后续安装操作使用。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `triggers` | 需要卸载的逻辑触发器集合，由位掩码和编号基值共同指定 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 所选触发器卸载成功 |
| `SbiRet::invalid_param` | 至少一个所选逻辑编号不小于 `trig_max`，或对应触发器尚未映射到硬件触发器 |

#### 启用触发器

```rust
fn enable_triggers(&self, triggers: TriggerMask) -> SbiRet;
```

启用当前 hart 上由 `triggers` 选中的已安装触发器。SBI 实现从保存的 `trig_state` 恢复硬件触发器的 `vs`、`vu`、`s` 和 `u` 模式使能位。

> 启用操作恢复原配置允许的执行模式，不会将所有模式使能位统一设置为 1。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `triggers` | 需要启用的逻辑触发器集合，由位掩码和编号基值共同指定 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 所选触发器启用成功 |
| `SbiRet::invalid_param` | 至少一个所选逻辑编号不小于 `trig_max`，或对应触发器尚未映射到硬件触发器 |

#### 禁用触发器

```rust
fn disable_triggers(&self, triggers: TriggerMask) -> SbiRet;
```

禁用当前 hart 上由 `triggers` 选中的已安装触发器。SBI 实现清除硬件触发器的 `vs`、`vu`、`s` 和 `u` 模式使能位，保留已安装的配置和逻辑编号映射。

> 禁用后可通过 `enable_triggers` 重新启用。若要释放触发器资源，应使用 `uninstall_triggers`。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `triggers` | 需要禁用的逻辑触发器集合，由位掩码和编号基值共同指定 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 所选触发器禁用成功 |
| `SbiRet::invalid_param` | 至少一个所选逻辑编号不小于 `trig_max`，或对应触发器尚未映射到硬件触发器 |

### MPXY 消息代理扩展

消息代理扩展允许特权态（S 态）系统软件通过 SBI 实现收发消息，为 RPMI 等消息协议提供统一的访问接口。

每个消息通道由 32 位的 `channel_id` 标识，通道属性描述其消息协议、最大数据长度和支持的功能。具体的消息编号、请求格式和响应格式由通道使用的消息协议规定。

消息收发使用每 hart 独立设置的共享内存。使用前，应先调用 `get_shmem_size` 查询容量，再调用 `set_shmem` 设置当前 hart 的消息缓冲区。

> 这块共享内存用于 S 态软件与 SBI 实现交换数据。SBI 实现与远端设备或固件之间的消息传输方式，由具体协议和平台决定。

本扩展的特型描述如下。

```rust
pub trait Mpxy {
    // Required methods
    fn get_shmem_size(&self) -> usize;
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet;
    fn get_channel_ids(&self, start_index: u32) -> SbiRet;
    fn read_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        output: SharedPtr<u8>,
    ) -> SbiRet;
    fn write_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        input: SharedPtr<u8>,
    ) -> SbiRet;
    fn send_message_with_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet;
    fn send_message_without_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet;
    fn get_notification_events(&self, channel_id: u32) -> SbiRet;
}
```

以上方法都需要由实现者提供。消息发送和通知获取功能可以按通道选择性支持，并通过通道的 `CHANNEL_CAPABILITY` 属性报告；未支持的操作应返回 `SbiRet::not_supported`。

> 属性读写接口存在调用约定差异：SBI 3.0 的属性读写调用只传入前三个整数参数，使用 `set_shmem` 配置的每 hart 缓冲区；RustSBI 的 `Mpxy` 特型额外接收 `output` 或 `input`，派生分派从 `a3`、`a4` 构造该物理地址。接入标准调用时，属性访问应使用 `set_shmem` 保存的缓冲区，不能将 `a3`、`a4` 视为标准调用提供的地址参数。

MPXY 扩展具有以下的函数。

#### 获取消息共享内存大小

```rust
fn get_shmem_size(&self) -> usize;
```

获取消息缓冲区所需的字节数，以便系统软件分配共享内存。

返回的大小应当在所有 hart 上相同，至少为 4096 字节，并且是 4096 字节的整数倍。它还必须不小于所有消息通道的 `MSG_DATA_MAX_LEN` 属性值。

> 本函数不要求预先调用 `set_shmem`。Rust 接口直接返回 `usize`，派生分派将它包装为 `SbiRet::success`，作为 SBI 调用结果。

本函数没有传入参数。

函数返回以下内容：

| 返回值 | 说明 |
|:----|:-----|
| `usize` | 每个 hart 的消息共享内存大小，单位为字节 |

#### 设置消息共享内存

```rust
fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet;
```

为当前 hart 设置消息共享内存，或停用已经设置的缓冲区。

启用时，`shmem` 指定的物理基地址必须按 4096 字节对齐，缓冲区容量应当等于 `get_shmem_size` 返回的大小。SBI 实现需要检查整个物理地址范围的访问权限。

> `SharedPtr<u8>` 只携带物理基地址的高、低部分，不携带缓冲区长度，也不会自动分配或映射内存。

`flags` 的低两位选择设置模式，其余位必须为 0。

| `flags` 值 | 模式 | 说明 |
|:----|:-----|:-----|
| 0 | `OVERWRITE` | 用新配置替换当前 hart 的共享内存配置 |
| 1 | `OVERWRITE-RETURN` | 替换配置，并在新缓冲区起始处依次写入旧物理基地址的低、高 XLEN 位 |

> 低两位为 2 或 3 的模式保留供未来使用。`OVERWRITE-RETURN` 可用于临时切换缓冲区：调用方先保存返回的旧地址，完成消息操作后，再恢复旧配置。

传入 `SharedPtr::new(usize::MAX, usize::MAX)` 时，表示停用当前 hart 的消息共享内存。需要停用时，可使用 `flags = 0`。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `shmem` | 消息共享内存的物理基地址；高、低部分均为 `usize::MAX` 时停用共享内存 |
| `flags` | 共享内存设置模式，0 表示替换，1 表示替换并保存旧地址，其余位必须为 0 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 已设置或停用共享内存 |
| `SbiRet::invalid_param` | `flags` 包含无效模式或保留位，或启用时物理基地址未按 4096 字节对齐 |
| `SbiRet::invalid_address` | 指定物理地址范围不满足 SBI 共享内存的访问要求 |
| `SbiRet::failed` | 设置过程中发生了未指定的错误 |

#### 获取消息通道编号

```rust
fn get_channel_ids(&self, start_index: u32) -> SbiRet;
```

将系统软件可以访问的消息通道编号写入当前 hart 的消息共享内存。

`start_index` 是通道列表中的起始下标，不是通道编号。返回数据由 32 位无符号整数构成，采用小端字节序，其布局如下。

| 偏移量 | 字段 | 说明 |
|:----|:-----|:-----|
| `0x00` | `REMAINING` | 本次返回后尚未提供的通道编号数量 |
| `0x04` | `RETURNED` | 本次写入的通道编号数量 |
| `0x08 + 4 * i` | 通道编号 | 通道列表中下标为 `start_index + i` 的编号，`i` 从 0 开始 |

> 如果 `REMAINING` 不为 0，调用方可将 `start_index` 增加 `RETURNED`，继续获取其余编号。

本函数的 `SbiRet::value` 总是为 0；通道编号和数量通过共享内存返回。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `start_index` | 本次开始读取的通道列表下标 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 已将通道编号和数量写入共享内存 |
| `SbiRet::invalid_param` | `start_index` 无效 |
| `SbiRet::no_shmem` | 当前 hart 尚未设置消息共享内存，或共享内存已停用 |
| `SbiRet::denied` | 不允许当前 hart 获取通道列表 |
| `SbiRet::failed` | 获取过程中发生了未指定的错误 |

#### 读取消息通道属性

```rust
fn read_attributes(
    &self,
    channel_id: u32,
    base_attribute_id: u32,
    attribute_count: u32,
    output: SharedPtr<u8>,
) -> SbiRet;
```

读取指定消息通道的一段连续属性，并通过 `output` 指定的物理缓冲区输出属性值。

每个属性值为 32 位，编号为 `base_attribute_id + i` 的属性保存在缓冲区偏移量 `4 * i` 处，采用小端字节序。输出缓冲区必须至少容纳 `attribute_count` 个属性值，并允许相应的写访问。

> 属性编号的最高位为 0 时，表示 MPXY 标准属性；最高位为 1 时，表示消息协议专用属性。两类属性必须分开读取，不能在同一次调用中跨越这两类编号范围。

常用的标准属性如下。

| 属性编号 | 名称 | 说明 |
|:----|:-----|:-----|
| `0x00` | `MSG_PROT_ID` | 消息协议编号 |
| `0x01` | `MSG_PROT_VERSION` | 消息协议版本 |
| `0x02` | `MSG_DATA_MAX_LEN` | 通道支持的最大消息数据长度，单位为字节 |
| `0x03` | `MSG_SEND_TIMEOUT` | 消息发送超时，单位为微秒 |
| `0x04` | `MSG_COMPLETION_TIMEOUT` | 消息发送并等待响应的总超时，单位为微秒 |
| `0x05` | `CHANNEL_CAPABILITY` | 通道功能位图，第 3、4、5 位分别表示支持带响应发送、无响应发送和获取通知 |
| `0x0B` | `EVENTS_STATE_CONTROL` | 通知事件统计的使能状态 |

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `channel_id` | 需要读取属性的消息通道编号 |
| `base_attribute_id` | 起始属性编号 |
| `attribute_count` | 连续读取的属性数量 |
| `output` | 存放 32 位属性值数组的物理缓冲区 |

标准 MPXY 属性读取调用可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 属性读取成功 |
| `SbiRet::invalid_param` | `attribute_count` 为 0、超出标准每 hart 缓冲区可容纳的 32 位属性数量，或 `base_attribute_id` 无效 |
| `SbiRet::not_supported` | `channel_id` 无效或不受支持 |
| `SbiRet::bad_range` | 指定范围内至少有一个属性不存在 |
| `SbiRet::no_shmem` | 标准调用所需的每 hart 共享内存未设置或已停用 |
| `SbiRet::failed` | 读取过程中发生了未指定的错误 |

> 上表中的缓冲区容量检查和 `no_shmem` 对应标准 SBI 调用。实现 RustSBI 的显式 `output` 参数时，还需校验该物理缓冲区的写访问；派生分派不会自动把它替换为 `set_shmem` 保存的地址。

#### 写入消息通道属性

```rust
fn write_attributes(
    &self,
    channel_id: u32,
    base_attribute_id: u32,
    attribute_count: u32,
    input: SharedPtr<u8>,
) -> SbiRet;
```

从 `input` 指定的物理缓冲区读取属性值，写入指定消息通道的一段连续属性。

调用方应将编号为 `base_attribute_id + i` 的 32 位属性值，以小端字节序放在缓冲区偏移量 `4 * i` 处。输入缓冲区必须包含 `attribute_count` 个属性值，并允许相应的读访问。

> 写入范围必须全部为可写属性。MPXY 标准属性和消息协议专用属性应当分开写入，不能在同一次调用中跨越两类编号范围。

部分属性具有写入前提。例如，启用 MSI 通知前，需要先配置有效的 MSI 目标地址。需要获取通知事件数量统计时，可在通道支持该功能的前提下，将 `EVENTS_STATE_CONTROL` 属性设为 1。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 4 个参数：

| 参数 | 说明 |
|:----|:-----|
| `channel_id` | 需要写入属性的消息通道编号 |
| `base_attribute_id` | 起始属性编号 |
| `attribute_count` | 连续写入的属性数量 |
| `input` | 存放待写入 32 位属性值数组的物理缓冲区 |

标准 MPXY 属性写入调用可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 属性写入成功 |
| `SbiRet::invalid_param` | `attribute_count` 为 0、超出标准每 hart 缓冲区可容纳的 32 位属性数量，或 `base_attribute_id` 无效 |
| `SbiRet::not_supported` | `channel_id` 无效或不受支持 |
| `SbiRet::bad_range` | 指定范围内有不存在或只读的属性，或范围同时覆盖标准属性和消息协议专用属性 |
| `SbiRet::no_shmem` | 标准调用所需的每 hart 共享内存未设置或已停用 |
| `SbiRet::denied` | 至少一个属性的写入前提未满足 |
| `SbiRet::failed` | 写入过程中发生了未指定的错误 |

> 上表中的缓冲区容量检查和 `no_shmem` 对应标准 SBI 调用。实现 RustSBI 的显式 `input` 参数时，还需校验该物理缓冲区的读访问；派生分派不会自动把它替换为 `set_shmem` 保存的地址。

#### 发送消息并等待响应

```rust
fn send_message_with_response(
    &self,
    channel_id: u32,
    message_id: u32,
    message_data_len: usize,
) -> SbiRet;
```

通过指定通道发送消息，并等待该通道返回响应。

调用前，系统软件应将请求数据放在当前 hart 消息共享内存的起始处。`message_data_len` 指定请求数据的字节数，不能超过该通道的 `MSG_DATA_MAX_LEN` 或消息共享内存的容量。

成功时，SBI 实现将响应数据写回同一缓冲区的起始处，并通过 `SbiRet::value` 返回响应数据的字节数。

> 请求和响应的数据格式均由消息协议规定。SBI 调用成功只表示消息已发送且已收到响应；具体操作的协议级结果应当从响应中读取。协议要求分次传输时，调用方需要组织多次消息发送。

本函数是通道的可选功能，`CHANNEL_CAPABILITY` 的第 3 位表示是否支持。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `channel_id` | 发送消息的通道编号 |
| `message_id` | 消息协议定义的消息编号 |
| `message_data_len` | 请求数据的字节数 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 消息已发送并收到响应，返回响应数据的字节数 |
| `SbiRet::invalid_param` | 请求长度超过通道允许的最大值，或超过当前 hart 消息共享内存的容量 |
| `SbiRet::not_supported` | 通道编号或消息编号无效、不受支持，或通道未实现本功能 |
| `SbiRet::no_shmem` | 当前 hart 尚未设置消息共享内存，或共享内存已停用 |
| `SbiRet::timeout` | 等待响应超时 |
| `SbiRet::io` | 消息传输发生 I/O 错误 |
| `SbiRet::failed` | 消息处理过程中发生了未指定的错误 |

#### 发送无需响应的消息

```rust
fn send_message_without_response(
    &self,
    channel_id: u32,
    message_id: u32,
    message_data_len: usize,
) -> SbiRet;
```

通过指定通道发送消息，发送成功后返回，不等待消息响应。

请求数据放在当前 hart 消息共享内存的起始处，长度由 `message_data_len` 指定。长度不能超过通道的 `MSG_DATA_MAX_LEN` 或消息共享内存的容量。

> 本函数用于协议规定无需响应的消息。它仍需完成消息发送，发送过程可能超时；若需要跟踪远端处理结果，应当使用协议规定的通知或其它查询消息。

本函数是通道的可选功能，`CHANNEL_CAPABILITY` 的第 4 位表示是否支持。

本函数通过 `SbiRet::error` 报告结果，成功时建议返回 `SbiRet::success(0)`。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `channel_id` | 发送消息的通道编号 |
| `message_id` | 消息协议定义的消息编号 |
| `message_data_len` | 请求数据的字节数 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 消息发送成功 |
| `SbiRet::invalid_param` | 请求长度超过通道允许的最大值，或超过当前 hart 消息共享内存的容量 |
| `SbiRet::not_supported` | 通道编号或消息编号无效、不受支持，或通道未实现本功能 |
| `SbiRet::no_shmem` | 当前 hart 尚未设置消息共享内存，或共享内存已停用 |
| `SbiRet::timeout` | 消息发送超时 |
| `SbiRet::io` | 消息传输发生 I/O 错误 |
| `SbiRet::failed` | 消息处理过程中发生了未指定的错误 |

#### 获取消息通道通知

```rust
fn get_notification_events(&self, channel_id: u32) -> SbiRet;
```

获取指定通道已经收到的通知事件，将通知数据写入当前 hart 的消息共享内存。

通知是异步产生的，具体格式由消息协议规定。平台可使用 MSI 或 SSE 通知系统软件有事件待处理，系统软件也可以调用本函数轮询。

通知数据始终从共享内存偏移量 `0x10` 开始存放。成功时，`SbiRet::value` 返回通知数据的字节数，不包含前 16 字节的事件统计区域。

事件统计由通道的 `EVENTS_STATE_CONTROL` 属性控制，默认关闭。若通道支持并启用统计，共享内存的前 16 字节使用以下布局，其中每个字段均为 32 位。

| 偏移量 | 字段 | 说明 |
|:----|:-----|:-----|
| `0x00` | `REMAINING` | SBI 实现中尚未返回的事件数量 |
| `0x04` | `RETURNED` | 本次返回的事件数量 |
| `0x08` | `LOST` | 因协议实现缓冲区容量有限而丢失的事件数量 |
| `0x0C` | `RESERVED` | 保留字段 |

> 若 `REMAINING` 不为 0，可继续调用本函数读取剩余事件。未支持或未启用事件统计时，这些统计字段的值未定义，但通知数据仍从 `0x10` 开始。

本函数是通道的可选功能，`CHANNEL_CAPABILITY` 的第 5 位表示是否支持获取通知，第 2 位表示是否支持事件统计。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `channel_id` | 需要获取通知的消息通道编号 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 通知获取成功，返回从偏移量 `0x10` 开始写入的通知数据字节数 |
| `SbiRet::not_supported` | 通道编号无效、不受支持，或通道未实现获取通知功能 |
| `SbiRet::no_shmem` | 当前 hart 尚未设置消息共享内存，或共享内存已停用 |
| `SbiRet::io` | 获取通知时发生 I/O 错误 |
| `SbiRet::failed` | 获取通知过程中发生了未指定的错误 |

## `EnvInfo` 特型

`EnvInfo` 特型为 SBI 基础扩展（BASE）提供处理器的厂商、架构和实现标识。模拟器和虚拟机监控程序可以通过这一特型提供客户机所见的处理器信息。

本特型描述如下。

```rust
pub trait EnvInfo {
    // Required methods
    fn mvendorid(&self) -> usize;
    fn marchid(&self) -> usize;
    fn mimpid(&self) -> usize;
}
```

启用 `machine` 特性且未指定 `EnvInfo` 实现时，派生宏生成的代码直接读取机器态标识寄存器。如果指定了 `EnvInfo` 实现，则使用该实现提供的信息。

> 这些标识描述的是处理器，不是 SBI 固件。SBI 实现编号、SBI 实现版本和 SBI 规范版本由基础扩展的其它函数提供，不应通过 `mimpid` 等方法代替。

### 获取厂商标识

```rust
fn mvendorid(&self) -> usize;
```

返回处理器厂商标识，其编码遵循 RISC-V `mvendorid` 寄存器的定义。非零值包含 JEDEC 厂商编号的编码；返回 0 表示未提供厂商标识。

本函数没有传入参数。

| 返回值 | 说明 |
|:----|:-----|
| `usize` | 向调用方提供的处理器厂商标识 |

### 获取架构标识

```rust
fn marchid(&self) -> usize;
```

返回处理器架构标识，其编码遵循 RISC-V `marchid` 寄存器的定义，用于标识 hart 的基础微架构。返回 0 表示未提供架构标识。

> 架构标识不是 ISA 扩展位图。调用方不能仅凭 `marchid` 判断处理器是否支持某个指令集扩展。

本函数没有传入参数。

| 返回值 | 说明 |
|:----|:-----|
| `usize` | 向调用方提供的处理器架构标识 |

### 获取实现标识

```rust
fn mimpid(&self) -> usize;
```

返回处理器实现标识，其编码遵循 RISC-V `mimpid` 寄存器的定义，用于区分处理器实现的版本。具体编码由处理器实现者定义，返回 0 表示未提供实现标识。

本函数没有传入参数。

| 返回值 | 说明 |
|:----|:-----|
| `usize` | 向调用方提供的处理器实现标识 |

## 结构体

`rustsbi` 在包的根模块中导出 `SbiRet`、`Physical`、`SharedPtr`、`HartMask` 和 `CounterMask`，用于描述 SBI 调用中的返回值、物理内存和资源集合。这些类型也可以通过 `rustsbi::spec::binary` 访问。

### SBI 返回值 `SbiRet`

`SbiRet` 表示一个 SBI 调用的错误码和返回数据，其定义如下。

```rust
#[repr(C)]
pub struct SbiRet<T = usize> {
    pub error: T,
    pub value: T,
}
```

通常使用默认的 `SbiRet<usize>`。在 SBI 调用约定中，`error` 对应返回时的 `a0` 寄存器，`value` 对应 `a1` 寄存器。

泛型参数 `T` 表示寄存器中的数值类型，使用构造方法时需满足 `rustsbi::spec::binary::SbiRegister` 约束。

| 字段 | 说明 |
|:----|:-----|
| `error` | SBI 错误码，0 表示成功，负数表示错误；使用 `usize` 时保留寄存器中的二进制表示 |
| `value` | 返回数据，其含义由具体的 SBI 函数定义 |

> 应先检查 `error`，再按相应函数的定义解释 `value`。部分函数在特定错误下也定义了 `value` 的含义；不能对所有 SBI 调用作统一假设。

#### 构造成功返回值

```rust
pub const fn success(value: T) -> Self;
```

构造一个 `error` 为 0、`value` 为指定值的 SBI 返回结果。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `value` | 本次调用的返回数据 |

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet<T>` | 表示调用成功的返回结果 |

对于不需要返回数据且规范要求 `value` 为 0 的函数，使用 `SbiRet::success(0)`。对于返回字节数、计数值或资源编号的函数，应填入相应的数据。

#### 构造错误返回值

`SbiRet` 提供以下无参数构造函数。它们将 `error` 设置为对应的 SBI 错误码，并将 `value` 初始化为 0。

| 构造函数 | 错误码 | 说明 |
|:----|:-----|:-----|
| `failed()` | -1 | 操作失败，未指定具体原因 |
| `not_supported()` | -2 | 操作或功能不受支持 |
| `invalid_param()` | -3 | 参数无效 |
| `denied()` | -4 | 操作被拒绝 |
| `invalid_address()` | -5 | 地址无效 |
| `already_available()` | -6 | 资源已经可用 |
| `already_started()` | -7 | 资源已经启动 |
| `already_stopped()` | -8 | 资源已经停止 |
| `no_shmem()` | -9 | 未设置所需的共享内存 |
| `invalid_state()` | -10 | 状态不允许执行该操作 |
| `bad_range()` | -11 | 指定范围无效 |
| `timeout()` | -12 | 操作超时 |
| `io()` | -13 | 发生 I/O 错误 |
| `denied_locked()` | -14 | 配置已锁定，拒绝修改 |

> 每个 SBI 函数允许返回的错误不同，应以相应函数的说明为准。若该函数在错误情况下要求返回附加数据，还需要按其定义设置 `value`。

#### 检查和转换返回值

对于 `SbiRet<usize>`，可以使用以下方法检查结果。

| 方法 | 返回值 | 说明 |
|:----|:-----|:-----|
| `is_ok()` | `bool` | 错误码为 0 时返回 `true` |
| `is_err()` | `bool` | 错误码不为 0 时返回 `true` |
| `into_result()` | `Result<usize, rustsbi::spec::binary::Error>` | 成功时返回 `Ok(value)`，失败时返回对应的错误 |

例如，可以将 SBI 返回值转换为 Rust 的 `Result` 后处理。

```rust
use rustsbi::SbiRet;

let ret: SbiRet = SbiRet::success(4);
assert_eq!(ret.into_result(), Ok(4));
```

`into_result()` 的错误分支只保留错误类型。如果具体函数在错误时通过 `value` 返回附加数据，应在转换前读取并保存该字段。

### 物理地址段 `Physical`

`Physical<P>` 描述一段物理地址连续的内存，包含字节数以及物理基地址的低、高部分。调试控制台的 `write` 和 `read` 方法分别使用 `Physical<&[u8]>` 和 `Physical<&mut [u8]>` 描述输入、输出缓冲区。

#### 构造物理地址段

```rust
pub const fn new(
    num_bytes: usize,
    phys_addr_lo: usize,
    phys_addr_hi: usize,
) -> Self;
```

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `num_bytes` | 物理地址段的字节数 |
| `phys_addr_lo` | 物理基地址的低 XLEN 位 |
| `phys_addr_hi` | 物理基地址的高 XLEN 位 |

| 返回值 | 说明 |
|:----|:-----|
| `Physical<P>` | 具有指定地址和长度的物理地址段描述 |

构造后，可分别使用 `num_bytes()`、`phys_addr_lo()` 和 `phys_addr_hi()` 读取这些信息。

```rust
use rustsbi::Physical;

let bytes = Physical::<&[u8]>::new(16, 0x8000_0000, 0);
assert_eq!(bytes.num_bytes(), 16);
```

> `Physical` 保存的是地址描述，不会将物理地址转换为 Rust 引用，也不会检查内存是否有效。实现者应在访问前检查地址范围、访问权限和长度，并按执行环境建立所需的地址映射。类型参数中的引用不代表已经取得该物理内存的 Rust 借用。

### 共享物理地址 `SharedPtr`

`SharedPtr<T>` 描述 SBI 实现与调用方共享的物理内存基地址。与 `Physical` 不同，它不保存运行时长度；所需空间和布局由相应扩展规定。

#### 构造共享物理地址

```rust
pub const fn new(phys_addr_lo: usize, phys_addr_hi: usize) -> Self;
```

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `phys_addr_lo` | 共享内存物理基地址的低 XLEN 位 |
| `phys_addr_hi` | 共享内存物理基地址的高 XLEN 位 |

| 返回值 | 说明 |
|:----|:-----|
| `SharedPtr<T>` | 具有指定物理地址的共享内存描述 |

构造后，可通过 `phys_addr_lo()` 和 `phys_addr_hi()` 读取地址的两个部分。例如，STA 扩展使用以下形式描述 64 字节的统计区域。

```rust
use rustsbi::SharedPtr;

let shmem = SharedPtr::<[u8; 64]>::new(0x8000_0000, 0);
assert_eq!(shmem.phys_addr_hi(), 0);
```

> 构造 `SharedPtr` 不会分配、清零或映射共享内存，也不会验证地址的对齐和访问权限。实现者必须遵守相应扩展的内存布局、访问顺序和生命周期要求。

部分扩展使用地址高、低部分均为 `usize::MAX` 的值禁用共享内存。这是对应 SBI 函数定义的特殊参数，不代表可访问的物理地址。

### hart 掩码 `HartMask`

`HartMask` 通过掩码和基准 hart 编号描述一组 hart。对于普通掩码，若第 `i` 位为 1，则选择编号为 `hart_mask_base + i` 的 hart。

#### 构造 hart 掩码

```rust
pub const fn from_mask_base(hart_mask: T, hart_mask_base: T) -> Self;
```

通常使用默认的 `HartMask<usize>`。

泛型构造方法要求 `T` 实现 `rustsbi::spec::binary::SbiRegister`；下文的查询、修改和遍历方法使用默认的 `usize` 类型。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `hart_mask` | 选择 hart 的位掩码 |
| `hart_mask_base` | 位掩码第 0 位对应的 hart 编号 |

| 返回值 | 说明 |
|:----|:-----|
| `HartMask<T>` | 指定的 hart 集合 |

例如，以下掩码选择编号为 4 和 6 的 hart。

```rust
use rustsbi::HartMask;

let harts = HartMask::from_mask_base(0b101, 4);
assert!(harts.has_bit(4));
assert!(!harts.has_bit(5));
assert!(harts.has_bit(6));
```

当 `hart_mask_base` 为 `usize::MAX` 时，SBI 约定忽略掩码并选择所有可用的 hart；可使用 `HartMask::all()` 构造这一特殊值。

#### 查询和修改 hart 掩码

| 方法 | 说明 |
|:----|:-----|
| `into_inner()` | 返回 `(hart_mask, hart_mask_base)` |
| `has_bit(hart_id)` | 判断掩码是否选择了指定的 hart 编号 |
| `insert(hart_id)` | 将指定 hart 加入集合；编号不能由该掩码表示时返回错误 |
| `remove(hart_id)` | 将指定 hart 从集合移除；操作不能由该掩码表示时返回错误 |
| `iter()` | 遍历掩码所表示的 hart 编号 |

> 掩码只描述 hart 编号的选择关系，不探测平台上实际存在的 hart。处理 `HartMask::all()` 时，实现者仍须依据平台的 hart 列表确定目标，不能通过遍历该掩码发现硬件。

### 计数器掩码 `CounterMask`

`CounterMask` 使用位掩码和基准计数器编号描述一组 PMU 计数器。若第 `i` 位为 1，则选择编号为 `counter_idx_base + i` 的计数器。

#### 构造计数器掩码

```rust
pub const fn from_mask_base(counter_idx_mask: T, counter_idx_base: T) -> Self;
```

通常使用默认的 `CounterMask<usize>`。

泛型构造方法要求 `T` 实现 `rustsbi::spec::binary::SbiRegister`；`has_bit` 方法仅由 `CounterMask<usize>` 提供。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `counter_idx_mask` | 选择计数器的位掩码 |
| `counter_idx_base` | 位掩码第 0 位对应的计数器编号 |

| 返回值 | 说明 |
|:----|:-----|
| `CounterMask<T>` | 指定的计数器集合 |

`has_bit(counter)` 可用于检查某个计数器编号是否被选中；`into_inner()` 返回 `(counter_idx_mask, counter_idx_base)`。

> `Pmu` 特型的方法接收的仍是独立的 `counter_idx_base` 和 `counter_idx_mask` 参数。它们的顺序与 `CounterMask::from_mask_base` 及 `into_inner()` 的顺序相反，传递时应予以区分。

### SBI 转发器 `Forward`

`Forward` 是一个无字段结构体。启用 `forward` 特性后，它通过 `sbi-rt` 将其实现的 SBI 方法转发给外层执行环境。

```rust
pub struct Forward;
```

`Forward` 实现了 `Console`、`Cppc`、`EnvInfo`、`Fence`、`Hsm`、`Ipi`、`Nacl`、`Pmu`、`Reset`、`Sse`、`Sta`、`Susp` 和 `Timer` 特型。可以通过派生宏的字段属性选择需要转发的功能。

例如，以下结构将 TIME 扩展和处理器标识信息交由外层执行环境提供。

```rust
use rustsbi::{Forward, RustSBI};

#[derive(RustSBI)]
struct ForwardSbi {
    #[rustsbi(timer, info)]
    forward: Forward,
}
```

> 使用前必须启用 `forward` 特性，并确保存在可以调用的外层 SBI 环境。未启用该特性时调用转发方法会触发 panic。
>
> 虚拟机监控程序应根据客户机的资源分配和地址空间选择可转发的调用；`Forward` 本身不负责客户机物理地址转换、hart 编号映射或访问权限检查。

`Forward` 也不会自动探测外层执行环境支持哪些扩展。需要按外层能力启用转发时，应先查询对应扩展，再通过动态组合和 `Option<Forward>` 表示其可用性。

## 辅助常量

`rustsbi` 提供以下公共常量。

| 常量 | 类型 | 说明 |
|:----|:-----|:-----|
| `rustsbi::VERSION` | `&str` | `rustsbi` 库的版本字符串 |
| `rustsbi::LOGO` | `&str` | RustSBI 的 ASCII 字符标志，可用于启动时的控制台输出 |

> `VERSION` 是库的版本，不是 SBI 规范版本，也不是基础扩展返回的整数形式的实现版本。

SBI 规范定义的扩展编号、函数编号、状态和标志常量由 `sbi-spec` 提供。`rustsbi` 将该库重导出为 `spec`，因此可以直接通过以下路径访问。

| 路径 | 说明 |
|:----|:-----|
| `rustsbi::spec::base` | 基础扩展的扩展编号、函数编号和探测结果常量 |
| `rustsbi::spec::hsm::hart_state` | hart 运行状态常量 |
| `rustsbi::spec::srst` | 系统重置类型、原因和函数编号 |
| `rustsbi::spec::pmu` | PMU 事件、计数器操作标志及共享内存相关常量 |
| `rustsbi::spec::binary` | SBI 返回值、错误类型及物理地址、资源掩码类型 |

其它扩展的常量位于 `spec` 下相应的扩展模块中。使用命名常量可以避免在实现中直接填写扩展编号和状态数值。

## 宏 `#[derive(RustSBI)]`

`#[derive(RustSBI)]` 将多个 SBI 扩展的实现组合为一个调用处理器。它为结构体实现 `RustSBI` 特型，负责根据扩展编号和函数编号分派调用，并处理 SBI 基础扩展的查询。

### 组合扩展实现

首先，为平台设备或软件对象实现相应的扩展特型，再将这些对象作为结构体的字段。

```rust
use rustsbi::{EnvInfo, Ipi, RustSBI, Timer};

#[derive(RustSBI)]
struct PlatformSbi<T: Timer, I: Ipi, E: EnvInfo> {
    timer: T,
    ipi: I,
    info: E,
}
```

派生宏通过字段名称识别扩展。上例的 `timer`、`ipi` 分别提供 TIME 和 IPI 扩展，`info` 为 BASE 扩展提供处理器标识。各个字段的类型必须实现对应的特型。

支持的字段名称如下。

| 字段名称 | 所需特型 | 说明 |
|:----|:-----|:-----|
| `console` 或 `dbcn` | `Console` | 调试控制台扩展 |
| `cppc` | `Cppc` | 协作处理器性能控制扩展 |
| `fence` 或 `rfnc` | `Fence` | 远程栅栏扩展 |
| `hsm` | `Hsm` | 硬件线程状态管理扩展 |
| `ipi` 或 `spi` | `Ipi` | 核间中断扩展 |
| `nacl` | `Nacl` | 嵌套虚拟化加速扩展 |
| `pmu` | `Pmu` | 性能监测扩展 |
| `reset` 或 `srst` | `Reset` | 系统重置扩展 |
| `sta` | `Sta` | 窃取时间统计扩展 |
| `susp` | `Susp` | 系统挂起扩展 |
| `timer` 或 `time` | `Timer` | 定时器扩展 |
| `sse` | `Sse` | S 态软件事件扩展 |
| `fwft` | `Fwft` | 固件特性扩展 |
| `dbtr` | `Dbtr` | 调试触发器扩展 |
| `mpxy` | `Mpxy` | 消息代理扩展 |
| `info` 或 `env_info` | `EnvInfo` | 基础扩展所需的处理器标识 |

未提供的扩展不会自动获得实现。基础扩展由派生宏统一提供；未启用 `machine` 特性时，必须提供 `EnvInfo` 字段。

> 默认的静态组合根据结构体是否包含相应字段报告扩展可用性，不检查 `Option` 在运行时是否为 `Some`。如果设备可能不存在，应使用下面介绍的动态组合方式。

### 指定字段用途

当字段名称与扩展名称不同时，可以使用 `#[rustsbi(...)]` 指定该字段提供的扩展。同一个对象可以实现多个扩展特型。

```rust
use rustsbi::{EnvInfo, Ipi, RustSBI, Timer};

#[derive(RustSBI)]
struct PlatformSbi<C: Timer + Ipi, E: EnvInfo> {
    #[rustsbi(timer, ipi)]
    clint: C,
    info: E,
    #[rustsbi(skip)]
    timer: usize,
}
```

上例将 `clint` 同时用于 TIME 和 IPI 扩展；`timer` 字段虽然使用了可识别的名称，但通过 `#[rustsbi(skip)]` 明确排除，不参与 SBI 调用分派。没有属性且名称不属于上表的字段也不会参与分派。

> 默认情况下，每种扩展只能指定一个实现字段。若要在运行时从多个实现中选择，应使用动态组合方式。

### 动态选择扩展

在结构体上增加 `#[rustsbi(dynamic)]`，可以为同一种扩展提供多个候选实现。通常使用 `Option<T>` 表示设备是否可用。

```rust
use rustsbi::{EnvInfo, RustSBI, Timer};

#[derive(RustSBI)]
#[rustsbi(dynamic)]
struct PlatformSbi<A: Timer, B: Timer, E: EnvInfo> {
    #[rustsbi(timer)]
    primary: Option<A>,
    #[rustsbi(timer)]
    secondary: Option<B>,
    info: E,
}
```

派生宏按照字段声明顺序选择第一个可用的实现。上例中，`primary` 为 `Some` 时使用它；否则继续检查 `secondary`。如果所有候选均不可用，则将该扩展报告为不可用，并对相应调用返回 `SbiRet::not_supported()`。

> 这一机制根据扩展是否可用进行选择。选定实现的方法返回错误后，不会继续尝试下一个候选实现。

### 处理 SBI 环境调用

派生宏实现的 `RustSBI` 特型描述如下。

```rust
pub trait RustSBI {
    fn handle_ecall(
        &self,
        extension: usize,
        function: usize,
        param: [usize; 6],
    ) -> SbiRet;
}
```

陷入处理程序识别出应由本执行环境处理的 SBI 环境调用后，将保存的参数交给 `handle_ecall`。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `extension` | 扩展编号，对应调用时的 `a7` 寄存器 |
| `function` | 函数编号，对应调用时的 `a6` 寄存器 |
| `param` | 调用参数，依次对应 `a0` 至 `a5` 寄存器 |

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet` | 调用结果；将 `error` 写回 `a0`，将 `value` 写回 `a1` |

```rust
use rustsbi::RustSBI;

fn dispatch(
    sbi: &impl RustSBI,
    extension: usize,
    function: usize,
    args: [usize; 6],
) -> (usize, usize) {
    let ret = sbi.handle_ecall(extension, function, args);
    (ret.error, ret.value)
}
```

调用返回后，陷入处理程序应按 SBI 调用约定保留其它寄存器，并将返回程序计数器前移 4 字节，跳过 `ecall` 指令。对于成功后不返回的操作，不执行这一返回流程。

> 派生宏提供调用分派，不负责安装陷入入口、保存寄存器或初始化平台设备。这些工作由固件、虚拟机监控程序或模拟器完成。

### 接入自定义扩展

自定义扩展使用 `Extension` 特型描述单个扩展的行为，并使用 `VendorSBI` 特型按扩展编号组织调用。

```rust
pub trait Extension {
    fn probe(&self) -> usize;
    fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet;
}

pub trait VendorSBI {
    fn probe_extension(&self, extension: usize) -> usize;
    fn handle_ecall(
        &self,
        extension: usize,
        function: usize,
        args: [usize; 6],
    ) -> SbiRet;
}
```

#### 探测单个自定义扩展

```rust
fn probe(&self) -> usize;
```

`Extension::probe` 查询该扩展是否可用，返回值用于 SBI 基础扩展的扩展探测操作。返回 0 表示不可用，非零值表示可用。

> 探测操作不应产生副作用。`VendorSBI` 派生宏除了在基础扩展查询时调用此方法，还会在实际分派调用前检查扩展是否可用。

本函数没有传入参数。

函数返回以下内容：

| 返回值 | 说明 |
|:----|:-----|
| 0 | 该自定义扩展不可用 |
| 非零值 | 该自定义扩展可用，作为基础扩展的探测结果返回 |

#### 处理单个自定义扩展的调用

```rust
fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet;
```

`Extension::handle` 处理已选中扩展内的一次函数调用。分派器已根据扩展编号选择了该对象，因此本函数只接收函数编号和调用参数。

返回数据和允许返回的错误由该自定义扩展定义。不支持的函数编号应返回 `SbiRet::not_supported()`。

> 派生宏会原样返回本函数的处理结果。即使处理结果为错误，也不会改用其它扩展对象重试。

函数传入 2 个参数：

| 参数 | 说明 |
|:----|:-----|
| `fid` | 扩展内的函数编号，对应调用时的 `a6` 寄存器 |
| `args` | 六个 SBI 参数，依次对应调用时的 `a0` 至 `a5` 寄存器 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 调用成功，`value` 的含义由该扩展的函数定义 |
| `SbiRet::not_supported` | 不支持指定的函数 |
| 该扩展定义的其它错误 | 按相应函数的规定报告处理失败的原因和附加数据 |

#### 按编号探测自定义扩展

```rust
fn probe_extension(&self, extension: usize) -> usize;
```

`VendorSBI::probe_extension` 查询扩展集合是否提供指定编号的扩展，并返回该扩展的探测结果。

由 `#[derive(VendorSBI)]` 为结构体生成的实现按字段绑定的扩展编号进行查询。编号匹配时，调用对应对象的 `Extension::probe`；未找到匹配编号时，返回 0。

本函数也应保持无副作用，以便 SBI 基础扩展随时查询扩展的可用性。

函数传入 1 个参数：

| 参数 | 说明 |
|:----|:-----|
| `extension` | 需要探测的 SBI 扩展编号 |

函数返回以下内容：

| 返回值 | 说明 |
|:----|:-----|
| 0 | 集合未提供该扩展，或对应扩展不可用 |
| 非零值 | 对应扩展的探测结果，表示扩展可用 |

#### 按编号分派自定义扩展调用

```rust
fn handle_ecall(
    &self,
    extension: usize,
    function: usize,
    args: [usize; 6],
) -> SbiRet;
```

`VendorSBI::handle_ecall` 根据扩展编号选择处理对象，再将函数编号和参数交给该对象处理。

由 `#[derive(VendorSBI)]` 为结构体生成的实现会先匹配扩展编号，再检查对应对象的 `Extension::probe`。只有探测结果非零时，才会调用 `Extension::handle`。

> 返回结果由选中的扩展决定，分派器不修改返回数据，也不会因处理失败而尝试其它扩展。未匹配到扩展编号或对应扩展不可用时，返回 `SbiRet::not_supported()`。

函数传入 3 个参数：

| 参数 | 说明 |
|:----|:-----|
| `extension` | 扩展编号，对应调用时的 `a7` 寄存器 |
| `function` | 扩展内的函数编号，对应调用时的 `a6` 寄存器 |
| `args` | 六个 SBI 参数，依次对应调用时的 `a0` 至 `a5` 寄存器 |

函数可能返回以下内容或错误：

| 返回值 | 说明 |
|:----|:-----|
| `SbiRet::success` | 对应扩展处理成功，返回其定义的数据 |
| `SbiRet::not_supported` | 集合未提供该扩展、对应扩展不可用，或不支持指定的函数 |
| 对应扩展返回的其它错误 | 原样返回选中扩展的处理结果 |

#### 组合自定义扩展

可以使用 `#[derive(VendorSBI)]` 为扩展集合生成探测和调用分派逻辑，再将其接入 `RustSBI`。

```rust
use rustsbi::{EnvInfo, Extension, RustSBI, VendorSBI};

// 示例编号；实际产品应使用为其分配的扩展编号。
const EID_EXAMPLE: usize = 0x0900_0000;

#[derive(VendorSBI)]
struct VendorExtensions<X: Extension> {
    #[rustsbi(extension(eid = EID_EXAMPLE))]
    example: X,
}

#[derive(RustSBI)]
struct PlatformSbi<E: EnvInfo, X: Extension> {
    info: E,
    #[rustsbi(vendor)]
    vendor: VendorExtensions<X>,
}
```

`#[rustsbi(vendor)]` 将一个实现 `VendorSBI` 的字段接入标准扩展分派器。每个结构体只能指定一个这样的字段；多个自定义扩展应放入同一个扩展集合。它同样适用于 `#[rustsbi(dynamic)]`。

> 自定义扩展编号必须能由 32 位表示，不能与已支持的标准扩展或遗留扩展编号冲突，同一集合也不能重复绑定编号。派生宏会在编译时检查这些约束；编号的实际分配仍须遵循 SBI 规范。

`Option<T>` 可用于表示可选的自定义扩展或扩展集合。`None` 表示不可用，对应的调用返回 `SbiRet::not_supported()`。

#### 使用枚举选择扩展集合

`#[derive(VendorSBI)]` 也支持枚举。每个变体必须包含一个未命名字段，其类型实现 `VendorSBI`。生成的实现将探测和调用分派交给当前变体保存的对象。

```rust
use rustsbi::VendorSBI;

#[derive(VendorSBI)]
enum SelectedVendor<A: VendorSBI, B: VendorSBI> {
    First(A),
    Second(B),
}
```

上例可用于在运行时选择不同平台的自定义扩展集合。`SelectedVendor::First` 使用 `A` 提供的集合，`SelectedVendor::Second` 使用 `B` 提供的集合；未选中的变体不参与探测或调用处理。

> 枚举类型可以作为 `#[rustsbi(vendor)]` 字段使用。枚举派生宏根据当前变体转发调用，不需要在枚举上设置 `#[rustsbi(dynamic)]`。

### 指定库的访问路径

如果依赖被重命名，或通过其它库重导出，可以在结构体上指定派生宏使用的库路径。

```rust
use rustsbi as sbi;

#[derive(sbi::RustSBI)]
#[rustsbi(crate = sbi)]
struct PlatformSbi<E: sbi::EnvInfo> {
    info: E,
}
```

`#[rustsbi(crate = ...)]` 也适用于 `VendorSBI` 派生宏，并可与 `#[rustsbi(dynamic)]` 一同用于 `RustSBI` 派生宏。
