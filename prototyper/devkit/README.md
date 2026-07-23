# Prototyper 开发工具

`cargo-prototyper` 是 Prototyper 固件、M-mode 测试、S-mode 测试和基准镜像的
宿主机侧构建入口。它统一决定目标架构、Cargo feature、链接脚本、产物命名和
QEMU 参数，目标代码不会依赖这个工具。

常用命令：

```console
cargo prototyper build
cargo prototyper build --image mtest
cargo prototyper build --image test
cargo prototyper build --image bench
cargo prototyper check --target riscv32imac-unknown-none-elf
cargo prototyper clippy
cargo prototyper run
```

四种 `--image` 角色分别选择 `linker/` 下独立维护的链接契约。这里的角色是镜像
语义，不等同于当前临时保留的 Cargo package 名。

普通 `cargo check` 仍可用于快速检查 Rust 源码；只有
`cargo prototyper build` 承诺生成采用受审链接布局、完成 ELF 到裸二进制转换并
使用稳定名称的可启动最终镜像。

不带额外参数的 `run` 会自动构建默认 S-mode 测试 payload、嵌入固件并启动
QEMU。只有运行自定义 payload 时才需要传 `--payload <BIN>`。

旧的 `cargo xtask prototyper`、`cargo xtask test` 和
`cargo xtask bench` 暂时保留，但只会显示弃用提示并转交给这个工具。
