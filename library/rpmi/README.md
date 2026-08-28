# RPMI definitions for Rust

`rpmi` is a `no_std` definitions crate for the
[ratified RISC-V Platform Management Interface v1.0](https://github.com/riscv-non-isa/riscv-rpmi/releases/tag/v1.0).
It provides service-group and service IDs, status and flag encodings, message
headers, and the small set of records whose layouts are defined by the
specification.

The crate intentionally does not implement a client, provider, mailbox,
shared-memory queue, MMIO access, cache maintenance, polling policy, or token
allocator. Those belong in transport- or runtime-specific crates.

```rust
use rpmi::{
    base,
    message::{MessageHeader, MessageType},
};

let header = MessageHeader::new(
    base::SERVICE_GROUP_ID,
    base::GET_SPEC_VERSION,
    MessageType::NormalRequest.bits(),
    0,
    7,
)
.unwrap();

assert_eq!(header.words(), [0x0004_0001, 0x0007_0000]);
```

`MessageHeader` and the record types model logical RPMI words. Byte ordering is
selected by a transport; the shared-memory transport uses little-endian words.
