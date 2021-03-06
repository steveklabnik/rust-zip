#![desc = "A simple rust library for reading and writing ZIP files"]
#![license = "MIT"]

#![feature(macro_rules)]

extern crate flate;

pub use self::fileinfo::{CompressionMethod, Deflate, Unknown, FileInfo};
pub use self::reader::ZipReader;

mod crc32;
pub mod maybe_utf8;
pub mod error;
pub mod format;
pub mod fileinfo;
pub mod reader;

