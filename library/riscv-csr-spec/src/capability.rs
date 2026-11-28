//! Bitwise Zicsr write capability markers for single-bit CSR fields.

use crate::sealed::Sealed;

/// A field with no atomic set or clear operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum None {}

/// A field that accepts [`Set`](crate::Set).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetOnly {}

/// A field that accepts [`Clear`](crate::Clear).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClearOnly {}

/// A field that accepts both [`Set`](crate::Set) and [`Clear`](crate::Clear).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetClear {}

impl Sealed for SetOnly {}
impl Sealed for ClearOnly {}
impl Sealed for SetClear {}

/// Sealed marker for bitfields supporting [`Set`](crate::Set).
pub trait AllowSet: Sealed {}
/// Sealed marker for bitfields supporting [`Clear`](crate::Clear).
pub trait AllowClear: Sealed {}
/// Sealed marker for single-bit fields supporting `|` mask composition.
pub trait AllowBitOp: Sealed {}

impl AllowSet for SetOnly {}
impl AllowSet for SetClear {}
impl AllowClear for ClearOnly {}
impl AllowClear for SetClear {}
impl AllowBitOp for SetOnly {}
impl AllowBitOp for ClearOnly {}
impl AllowBitOp for SetClear {}
