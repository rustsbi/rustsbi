//! RISC-V Platform Management Interface (RPMI) message protocol and
//! shared-memory mailbox transport.
//!
//! RPMI is the RISC-V standard interface for platform management between
//! an application processor (AP) and a platform management processor
//! (PuC), covering power, clock, voltage, reset, suspend, RTC and other
//! platform services. This crate provides:
//!
//! - [`message`]: the RPMI message header, message types, error codes and
//!   service group identifiers (RISC-V RPMI spec `message-protocol.adoc`
//!   and `service-groups.adoc`);
//! - [`smq`]: the shared-memory queue (SMQ) transport with the four
//!   A2P/P2A request and acknowledgement rings;
//! - [`mailbox`]: a mailbox controller offering the normal (request +
//!   response) and posted (fire-and-forget) request patterns.
//!
//! The reference implementation this crate mirrors is OpenSBI
//! `lib/utils/mailbox/{rpmi_mailbox.c, fdt_mailbox_rpmi_shmem.c}` and
//! `include/sbi_utils/mailbox/{rpmi_msgprot.h, mailbox.h}`.

#![no_std]

pub mod client;
pub mod mailbox;
pub mod message;
pub mod smq;

pub use client::BaseClient;
pub use mailbox::RpmiMailbox;
pub use message::{Error, MessageHeader, MessageType};
pub use smq::{Le32, SmqQueue};
