//! Chapter 3. Binary Encoding.

// SBI return value and error codes.
mod sbi_ret;
pub use sbi_ret::{Error, SbiRegister, SbiRet, id::*};

// Masks.
mod mask_commons;
pub use mask_commons::MaskError;

mod counter_mask;
mod hart_mask;
mod trigger_mask;
pub use counter_mask::CounterMask;
pub use hart_mask::{HartIds, HartMask};
pub use trigger_mask::TriggerMask;

// Pointers.
mod physical_slice;
mod shared_physical_ptr;
pub use physical_slice::Physical;
pub use shared_physical_ptr::SharedPtr;
