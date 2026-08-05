//! Rust support for RISC-V Advanced Interrupt Architecture (AIA).
//!
//! This crate follows _The RISC-V Advanced Interrupt Architecture_ specification, Version 1.0, Revised 20250312.

#![no_std]

mod iid;

pub mod geilen;
pub mod peripheral;
pub mod register;

pub use crate::iid::{Iid, MajorIid};
