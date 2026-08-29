//! Layout constants for the RPMI shared-memory transport.
//!
//! This module deliberately defines no queue algorithm, volatile-access type,
//! synchronization primitive, timeout, or doorbell operation. Those choices
//! belong to a transport implementation. Shared-memory words are serialized in
//! little-endian order by the transport.

/// Minimum queue-slot size in bytes.
///
/// A slot size must also be a power of two, and each slot's physical address
/// must be aligned to that size.
pub const MIN_SLOT_SIZE: usize = 64;
/// Slot containing the head index, updated only by the consumer.
///
/// Queue indices are relative to the message-slot array: index zero denotes
/// physical slot [`MESSAGE_SLOT_START`].
pub const HEAD_SLOT: usize = 0;
/// Slot containing the tail index, updated only by the producer.
///
/// Queue indices are relative to the message-slot array: index zero denotes
/// physical slot [`MESSAGE_SLOT_START`].
pub const TAIL_SLOT: usize = 1;
/// First slot available for messages.
pub const MESSAGE_SLOT_START: usize = 2;
/// Byte offset of the queue index within a head or tail slot.
pub const INDEX_OFFSET: usize = 0;
/// Size of a queue index in bytes.
pub const INDEX_SIZE: usize = 4;
/// Doorbell-interrupt request flag.
pub const DOORBELL_REQUEST: u8 = 0x08;
