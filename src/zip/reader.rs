use std::io::File;
use std::io::{Reader, Writer, Seek, SeekSet, SeekEnd};
use std::io::{IoResult, IoError, InvalidInput};
use std::iter;
use std::iter::range_inclusive;
use std::path::BytesContainer;
use error;
use error::ZipError;
use maybe_utf8::MaybeUTF8;
use flate;
use crc32;
use format;
use fileinfo;
use fileinfo::{CompressionMethod, FileInfo};

pub struct ZipReader<R> {
    reader: R,
    end_record: format::EndOfCentralDirectoryRecord,
}

pub struct Files<'a, R:'a> {
    zip_reader: &'a mut ZipReader<R>,
    current_entry: u16,
    current_offset: u64,
}

impl<'a, R:Reader+Seek> Iterator<Result<FileInfo, ZipError>> for Files<'a, R> {
    fn next(&mut self) -> Option<Result<FileInfo, ZipError>> {
        if self.current_entry < self.zip_reader.end_record.total_entry_count {
            match self.zip_reader.reader.seek(self.current_offset as i64, SeekSet) {
                Ok(()) => {}
                Err(err) => { return Some(Err(error::SomeIoError(err))); }
            }
            let h = match format::CentralDirectoryHeader::read(&mut self.zip_reader.reader) {
                Ok(h) => h,
                Err(err) => { return Some(Err(err)); }
            };
            let info = FileInfo::from_cdh(&h);
            self.current_entry += 1;
            self.current_offset += h.total_size() as u64;
            Some(Ok(info))
        } else {
            None
        }
    }
}

impl ZipReader<File> {
    pub fn open(path: &Path) -> Result<ZipReader<File>, ZipError> {
        ZipReader::new(try_io!(File::open(path)))
    }
}

impl<R:Reader+Seek> ZipReader<R> {
    pub fn new(reader: R) -> Result<ZipReader<R>, ZipError> {
        // find the End of Central Directory record, looking backwards from the end of the file
        let mut r = reader;
        try_io!(r.seek(0, SeekEnd));
        let file_size = try_io!(r.tell());
        let mut end_record_offset : Option<u64> = None;
        for i in range_inclusive(4, file_size) {
            let offset = file_size - i;
            try_io!(r.seek(offset as i64, SeekSet));

            let sig = try_io!(r.read_le_u32());

            // TODO: check for false positives here
            if sig == format::EOCDR_SIGNATURE {
                end_record_offset = Some(offset);
                break;
            }

        }

        match end_record_offset {
            Some(offset) => {
                try_io!(r.seek(offset as i64, SeekSet));
                let e = try!(format::EndOfCentralDirectoryRecord::read(&mut r));
                Ok(ZipReader {reader: r, end_record: e})
            },
            None => Err(error::NotAZipFile)
        }
    }

    pub fn files_raw<'a>(&'a mut self) -> Files<'a, R> {
        let cdr_offset = self.end_record.central_directory_offset;
        Files {
            zip_reader: self,
            current_entry: 0,
            current_offset: cdr_offset as u64
        }
    }

    pub fn files<'a>(&'a mut self) -> iter::Map<Result<FileInfo, ZipError>, FileInfo,
                                                Files<'a, R>> {
        self.files_raw().map(|fileinfo_or_err| fileinfo_or_err.unwrap())
    }

    pub fn file_names<'a>(&'a mut self) -> iter::Map<Result<FileInfo, ZipError>, MaybeUTF8,
                                                     Files<'a, R>> {
        self.files_raw().map(|fileinfo_or_err| fileinfo_or_err.unwrap().name)
    }

    pub fn info<T:BytesContainer>(&mut self, name: T) -> Result<FileInfo, ZipError> {
        for i in self.files() {
            if i.name.equiv(&name) {
                return Ok(i);
            }
        }
        Err(error::FileNotFoundInArchive)
    }

    // TODO: Create a Reader for the cases when you don't want to decompress the whole file
    pub fn read(&mut self, f: &FileInfo) -> Result<Vec<u8>, ZipError> {
        try_io!(self.reader.seek(f.local_file_header_offset as i64, SeekSet));
        let h = try!(format::LocalFileHeader::read(&mut self.reader));
        let file_offset = f.local_file_header_offset as i64 + h.total_size() as i64;

        let result =
            match CompressionMethod::from_u16(h.compression_method) {
                fileinfo::Store => self.read_stored_file(file_offset, h.uncompressed_size),
                fileinfo::Deflate => self.read_deflated_file(file_offset, h.compressed_size, h.uncompressed_size),
                _ => panic!()
            };
        let result = try_io!(result);

        // Check the CRC32 of the result against the one stored in the header
        let crc = crc32::crc32(result.as_slice());

        if crc == h.crc32 { Ok(result) }
        else { Err(error::CrcError) }
    }

    fn read_stored_file(&mut self, pos: i64, uncompressed_size: u32) -> IoResult<Vec<u8>> {
        try!(self.reader.seek(pos, SeekSet));
        self.reader.read_exact(uncompressed_size as uint)
    }

    fn read_deflated_file(&mut self, pos: i64, compressed_size: u32, uncompressed_size: u32) -> IoResult<Vec<u8>> {
        try!(self.reader.seek(pos, SeekSet));
        let compressed_bytes = try!(self.reader.read_exact(compressed_size as uint));
        let uncompressed_bytes = match flate::inflate_bytes(compressed_bytes.as_slice()) {
            Some(bytes) => bytes,
            None => return Err(IoError { kind: InvalidInput, desc: "decompression failure", detail: None })
        };
        assert!(uncompressed_bytes.len() as u32 == uncompressed_size);
        // FIXME try not to copy the buffer, or switch to the incremental fashion
        Ok(uncompressed_bytes.as_slice().to_vec())
    }

    // when we make read return a Reader, we will be able to loop here and copy
    // blocks of a fixed size from Reader to Writer
    pub fn extract<T:Writer>(&mut self, f: &FileInfo, writer: &mut T) -> Result<(), ZipError> {
        match self.read(f) {
            Ok(bytes) => { try_io!(writer.write(bytes.as_slice())); Ok(()) },
            Err(x) => Err(x)
        }
    }

}

