//! A list of possible errors.

#![macro_escape]

use std::fmt;
use std::io::IoError;

/// A list of possible errors. This is a supetset of `std::Io::IoError`.
#[deriving(PartialEq,Clone)]
pub enum ZipError {
    SomeIoError(IoError),
    NotAZipFile,
    CrcError,
    FileNotFoundInArchive,
    InvalidSignature(u32),
    NonUTF8Field,
    TooLongField,
}

impl fmt::Show for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SomeIoError(ref e) => e.fmt(f),
            NotAZipFile => "not a ZIP file".fmt(f),
            CrcError => "CRC mismatch".fmt(f),
            FileNotFoundInArchive => "file not found in archive".fmt(f),
            InvalidSignature(magic) => write!(f, "invalid ZIP signature {:#08x}", magic),
            NonUTF8Field => "file name or comment is set to UTF-8 encoded but it isn't".fmt(f),
            TooLongField => "file name, comment or extra field is too long (> 64KB)".fmt(f),
        }
    }
}

pub type ZipResult<T> = Result<T, ZipError>;

macro_rules! try_io(
    ($e:expr) => (try!($e.map_err(::error::SomeIoError)))
)

