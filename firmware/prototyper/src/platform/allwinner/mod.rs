//! Allwinner platform policy.
//!
//! V821 discovery, boot0 handshakes, and one-time platform preparation live here.
//! Acquired cache, USB, and hart-wakeup devices live in their functional driver modules.

pub(crate) mod v821;
