# QEMU example support using RustSBI

Compile and run with:

```shell
just run
```

Expected output should be:

```shell
   Compiling rustsbi-qemu v0.1.0 (.../rustsbi/platform/qemu)
    Finished dev [unoptimized + debuginfo] target(s) in 1.62s
[rustsbi] RustSBI version 0.1.1
.______       __    __      _______.___________.  _______..______   __
|   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
|  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
|      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
|  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
| _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|

[rustsbi] Platform: QEMU (Version 0.1.0)
[rustsbi] misa: RV64ACDFIMSU
[rustsbi] mideleg: 0x222
[rustsbi] medeleg: 0xb1ab
[rustsbi-dtb] Hart count: cluster0 with 2 cores
[rustsbi] Kernel entry: 0x80200000
[rustsbi-panic] panicked at 'invalid instruction, mepc: 0000000080200000, instruction: 0000000000000000', platform/qemu/src/main.rs:456:17
[rustsbi-panic] system shutdown scheduled due to RustSBI panic
```

Error 'invalid instruction' is expected, that means you should install
your kernel here at `0x80200000`.
