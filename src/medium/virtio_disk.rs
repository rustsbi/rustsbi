use axio::{self as io};
use alloc::{string::String, vec::Vec};

/// Returns the current working directory as a [`String`].
pub fn current_dir() -> io::Result<String> {
    axfs::api::current_dir()
}

/// Read the entire contents of a file into a bytes vector.
pub fn read(path: &str) -> io::Result<Vec<u8>> {
    axfs::api::read(path)
}

/// Read the entire contents of a file into a string.
pub fn read_to_string(path: &str) -> io::Result<String> {
    axfs::api::read_to_string(path)
}