# RustSBI platforms

Currently supported platforms:

| Platform | Legacy | Base | IPI | Timer | RFENCE | HSM | SRST | Note |
|:---------|:------:|:----:|:---:|:-----:|:------:|:---:|:----:|:-----|
| [Kendryte K210](./k210) | √ | √ | √ | √ | P | P | P | Privileged spec version: 1.9.1 |
| [QEMU](./qemu)          | √ | √ | √ | √ | P | P | √ | - |

P: Pending

## Notes

These platforms implementations are only for reference.
Although binaries are released along with RustSBI library itself,
platform developers should consider using RustSBI as a library,
other than adding code into forked project's 'platforms' and make a pull request.

A reference platform implementation using RustSBI can be found at [shady831213/terminus_bl](https://github.com/shady831213/terminus_bl).
