//! Validated byte ranges and the shared page traversal policy.

use crate::cfg::{PAGE_SIZE, TLB_FLUSH_LIMIT};
use runtime::{Result, rustsbi::SbiRet};

#[derive(Clone, Copy, Debug)]
pub(super) enum AddressRange {
    All,
    Pages { start: usize, size_bytes: usize },
}

impl AddressRange {
    pub(super) fn new(start: usize, size_bytes: usize) -> Result<Self, SbiRet> {
        if (start == 0 && size_bytes == 0) || size_bytes == usize::MAX {
            return Ok(Self::All);
        }
        if start & (PAGE_SIZE - 1) != 0 || start.checked_add(size_bytes).is_none() {
            return Err(SbiRet::invalid_address());
        }
        if size_bytes > TLB_FLUSH_LIMIT {
            Ok(Self::All)
        } else {
            Ok(Self::Pages { start, size_bytes })
        }
    }

    pub(super) fn for_each(self, all_fn: impl FnOnce(), mut address_fn: impl FnMut(usize)) {
        match self {
            Self::All => all_fn(),
            Self::Pages { start, size_bytes } => {
                for offset in (0..size_bytes).step_by(PAGE_SIZE) {
                    address_fn(start + offset);
                }
            }
        }
    }
}
