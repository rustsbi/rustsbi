#![feature(alloc_error_handler)]
#![forbid(unsafe_code)]
#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate log;

mod boot;
mod cfg;
mod devicetree;
mod driver;
mod fail;
mod heap;
mod next_stage;
mod platform;
mod sbi;
