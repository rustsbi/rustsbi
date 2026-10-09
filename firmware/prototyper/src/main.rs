#![feature(alloc_error_handler)]
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
mod firmware;
mod heap;
mod next_stage;
mod platform;
mod riscv;
mod sbi;
