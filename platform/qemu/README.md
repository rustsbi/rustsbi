# QEMU example support using RustSBI

Compile and run with:

```shell
just run
```

When running `just run`, the test kernel will build and run. Expected output should be:

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
<< Test-kernel: Hart id = 0, DTB physical address = 0x1020
>> Test-kernel: Trigger illegal exception
<< Test-kernel: Illegal exception delegate success
<< Test-kernel: SBI test SUCCESS, shutdown
```
