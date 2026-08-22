#![doc = include_str!("../README.md")]
#![no_std]

#[macro_use]
mod macros;

mod iid;

pub mod csrind;
pub mod geilen;
pub mod peripheral;
pub mod register;

pub use crate::iid::{Iid, MajorIid};
