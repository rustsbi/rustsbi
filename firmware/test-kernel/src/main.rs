#![no_std]
#![no_main]
#![allow(static_mut_refs)]

#[macro_use]
extern crate rcore_console;

mod boot;
mod console;
mod misaligned;
mod platform;
mod pmu;
mod reset;
mod rfence;

mod trap;
