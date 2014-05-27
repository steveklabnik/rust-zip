/// Internal format stuffs.

use std::io::{IoResult, IoError, InvalidInput};
use std::str; // TODO: look into std::ascii to see if it's a better fit
use std::fmt;

#[deriving(Clone)]
pub struct MsdosDateTime {
    time: u16,
    date: u16,
}

impl MsdosDateTime {
    pub fn new(year: uint, month: uint, day: uint,
               hour: uint, minute: uint, second: uint) -> MsdosDateTime {
        // XXX no strict error check
        let year = year - 1980;
        MsdosDateTime {
            time: (((hour & 0b11111) << 11) |
                   ((minute & 0b111111) << 5) |
                   ((second & 0b111111) >> 1)) as u16,
            date: (((year & 0b1111111) << 9) |
                   ((month & 0b1111) << 5) |
                   (day & 0b11111)) as u16,
        }
    }

    pub fn zero() -> MsdosDateTime {
        MsdosDateTime { time: 0, date: 0 }
    }

    pub fn year  (&self) -> uint { ((self.date >>  9) & 0b1111111) as uint + 1980 }
    pub fn month (&self) -> uint { ((self.date >>  5) &    0b1111) as uint }
    pub fn day   (&self) -> uint { ( self.date        &   0b11111) as uint }
    pub fn hour  (&self) -> uint { ((self.time >> 11) &   0b11111) as uint }
    pub fn minute(&self) -> uint { ((self.time >>  5) &  0b111111) as uint }
    pub fn second(&self) -> uint { ((self.time <<  1) &  0b111111) as uint }

    pub fn to_tuple(&self) -> (uint, uint, uint, uint, uint, uint) {
        (self.year(), self.month(), self.day(), self.hour(), self.minute(), self.second())
    }

    pub fn read<T:Reader>(r: &mut T) -> IoResult<MsdosDateTime> {
        let time = try!(r.read_le_u16());
        let date = try!(r.read_le_u16());
        Ok(MsdosDateTime { time: time, date: date })
    }

    pub fn write<T:Writer>(&self, w: &mut T) -> IoResult<()> {
        try!(w.write_le_u16(self.time));
        try!(w.write_le_u16(self.date));
        Ok(())
    }
}

impl fmt::Show for MsdosDateTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{:02}-{:02} {:02}:{:02}:{:02}",
               self.year(), self.month(), self.day(), self.hour(), self.minute(), self.second())
    }
}

fn invalid_signature<T>() -> IoResult<T> {
    Err(IoError { kind: InvalidInput, desc: "invalid signature", detail: None })
}

//  http://www.pkware.com/documents/casestudies/APPNOTE.TXT
//
//  4.3.6 Overall .ZIP file format:
//
//  [local file header 1]
//  [encryption header 1]
//  [file data 1]
//  [data descriptor 1]
//  . 
//  .
//  .
//  [local file header n]
//  [encryption header n]
//  [file data n]
//  [data descriptor n]
//  [archive decryption header] 
//  [archive extra data record] 
//  [central directory header 1]
//  .
//  .
//  .
//  [central directory header n]
//  [zip64 end of central directory record]
//  [zip64 end of central directory locator] 
//  [end of central directory record]


// ==== LOCAL FILE HEADER ====

pub static LFH_SIGNATURE: u32 = 0x04034b50;

pub struct LocalFileHeader {
    pub version_needed_to_extract: u16,
    pub general_purpose_bit_flag:  u16,
    pub compression_method:        u16,
    pub last_modified_datetime:    MsdosDateTime,
    pub crc32:                     u32,
    pub compressed_size:           u32,
    pub uncompressed_size:         u32,
    pub file_name_length:          u16,
    pub extra_field_length:        u16,
    pub file_name:                 String,
    pub extra_field:               Vec<u8>
}

impl LocalFileHeader {
    // -- header property getters
    
    // see section 4.4.4 of APPNOTE.TXT for more info about these flags
    pub fn is_encrypted(&self) -> bool               { (self.general_purpose_bit_flag &    1) != 0 } 
    pub fn has_data_descriptor(&self) -> bool        { (self.general_purpose_bit_flag &    8) != 0 }
    pub fn is_compressed_patched_data(&self) -> bool { (self.general_purpose_bit_flag &   32) != 0 }
    pub fn uses_strong_encryption(&self) -> bool     { (self.general_purpose_bit_flag &   64) != 0 }
    pub fn has_UTF8_name(&self) -> bool              { (self.general_purpose_bit_flag & 2048) != 0 }
    pub fn uses_masking(&self) -> bool               { (self.general_purpose_bit_flag & 8192) != 0 }

    pub fn total_size(&self) -> int {
        let local_file_header_fixed_size = 30;
        local_file_header_fixed_size + (self.file_name_length as int) + (self.extra_field_length as int) 
    }

    // -- constructors
    pub fn new() -> LocalFileHeader {
        LocalFileHeader{
            version_needed_to_extract: 0,
            general_purpose_bit_flag: 0,
            compression_method: 0,
            last_modified_datetime: MsdosDateTime::zero(),
            crc32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            file_name_length: 0,
            extra_field_length: 0,
            file_name: String::new(),
            extra_field: Vec::new()
        }
    }

    // reads a LocalFileHeader from the current position of the reader r
    pub fn read<T:Reader>(r: &mut T) -> IoResult<LocalFileHeader> {
        let mut h = LocalFileHeader::new();

        if try!(r.read_le_u32()) != LFH_SIGNATURE {
            return invalid_signature();
        }

        h.version_needed_to_extract = try!(r.read_le_u16());
        h.general_purpose_bit_flag = try!(r.read_le_u16());
        h.compression_method = try!(r.read_le_u16());
        h.last_modified_datetime = try!(MsdosDateTime::read(r));
        h.crc32 = try!(r.read_le_u32());
        h.compressed_size = try!(r.read_le_u32());
        h.uncompressed_size = try!(r.read_le_u32());
        h.file_name_length = try!(r.read_le_u16());
        h.extra_field_length = try!(r.read_le_u16());
        h.file_name = str::from_utf8_owned(try!(r.read_exact(h.file_name_length as uint))).unwrap();
        h.extra_field = try!(r.read_exact(h.extra_field_length as uint));

        // check for some things we don't support (yet?)
        assert!(!h.is_encrypted());
        assert!(!h.is_compressed_patched_data());
        assert!(!h.has_data_descriptor());
        assert!(!h.uses_strong_encryption());
        assert!(!h.uses_masking());

        Ok(h)
    }

    pub fn write<T:Writer>(&self, w: &mut T) -> IoResult<()> {
        try!(w.write_le_u32(LFH_SIGNATURE));
        try!(w.write_le_u16(self.version_needed_to_extract));
        try!(w.write_le_u16(self.general_purpose_bit_flag));
        try!(w.write_le_u16(self.compression_method));
        try!(self.last_modified_datetime.write(w));
        try!(w.write_le_u32(self.crc32));
        try!(w.write_le_u32(self.compressed_size));
        try!(w.write_le_u32(self.uncompressed_size));
        try!(w.write_le_u16(self.file_name_length));
        try!(w.write_le_u16(self.extra_field_length));
        try!(w.write(self.file_name.as_bytes()));
        try!(w.write(self.extra_field.as_slice()));
        Ok(())
    }

    // for debug purposes
    pub fn print(&self) {
        println!("version_needed_to_extract: {:#04x}", self.version_needed_to_extract);
        println!("general_purpose_bit_flag: {:#04x}", self.general_purpose_bit_flag);
        println!("compression_method: {:#04x}", self.compression_method);
        println!("last_modified_datetime: {}", self.last_modified_datetime);
        println!("crc32: {:#08x}", self.crc32);
        println!("compressed_size: {}", self.compressed_size);
        println!("uncompressed_size: {}", self.uncompressed_size);
        println!("file_name_length: {}", self.file_name_length);
        println!("extra_field_length: {}", self.extra_field_length);
        println!("file_name: {}", self.file_name);
        println!("extra_field: {:?}", self.extra_field);

        println!("FLAGS: ");
        println!("  is encrypted: {}", self.is_encrypted());
        println!("  has data descriptor: {}", self.has_data_descriptor());
        println!("  is compressed patched data: {}", self.is_compressed_patched_data());
        println!("  uses strong encryption: {}", self.uses_strong_encryption());
        println!("  has UFT8 name: {}", self.has_UTF8_name());
        println!("  uses masking: {}", self.uses_masking());
    }
}

// TODO: Add support for data descriptor section after the file contents (typically used when the zip file
// writer doesn't know the file size beforehand, because it's receiving a stream of data or something)

pub static DD_SIGNATURE: u32 = 0x08074b50;

pub struct DataDescriptor {
    pub signature_present: bool, // not standard but sometimes present
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

// ==== CENTRAL DIRECTORY HEADER ====

pub static CDH_SIGNATURE: u32 = 0x02014b50;

pub struct CentralDirectoryHeader {
    pub version_made_by: u16,
    pub version_needed_to_extract: u16,
    pub general_purpose_bit_flag: u16,
    pub compression_method: u16,
    pub last_modified_datetime: MsdosDateTime,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub file_name_length: u16,
    pub extra_field_length: u16,
    pub file_comment_length: u16,
    pub disk_number_start: u16,
    pub internal_file_attributes: u16,
    pub external_file_attributes: u32,
    pub relative_offset_of_local_header: u32,
    pub file_name: String,
    pub extra_field: Vec<u8>,
    pub file_comment: String,
}

impl CentralDirectoryHeader {
    pub fn is_encrypted(&self) -> bool               { (self.general_purpose_bit_flag &    1) != 0 } 
    pub fn has_data_descriptor(&self) -> bool        { (self.general_purpose_bit_flag &    8) != 0 }
    pub fn is_compressed_patched_data(&self) -> bool { (self.general_purpose_bit_flag &   32) != 0 }
    pub fn uses_strong_encryption(&self) -> bool     { (self.general_purpose_bit_flag &   64) != 0 }
    pub fn has_UTF8_name(&self) -> bool              { (self.general_purpose_bit_flag & 2048) != 0 }
    pub fn uses_masking(&self) -> bool               { (self.general_purpose_bit_flag & 8192) != 0 }

    pub fn total_size(&self) -> int { 
        let central_directory_header_fixed_size = 46;
        central_directory_header_fixed_size
            + (self.file_name_length as int)
            + (self.extra_field_length as int)
            + (self.file_comment_length as int)
    }


    pub fn new() -> CentralDirectoryHeader {
        CentralDirectoryHeader {
            version_made_by: 0,
            version_needed_to_extract: 0,
            general_purpose_bit_flag: 0,
            compression_method: 0,
            last_modified_datetime: MsdosDateTime::zero(),
            crc32: 0,
            compressed_size: 0,
            uncompressed_size: 0,
            file_name_length: 0,
            extra_field_length: 0,
            file_comment_length: 0,
            disk_number_start: 0,
            internal_file_attributes: 0,
            external_file_attributes: 0,
            relative_offset_of_local_header: 0,
            file_name: String::new(),
            extra_field: Vec::new(),
            file_comment: String::new(),
        }
    }

    // reads a CentralDirectoryHeader from the current position of the reader r
    pub fn read<T:Reader>(r: &mut T) -> IoResult<CentralDirectoryHeader> {
        let mut h = CentralDirectoryHeader::new();

        if try!(r.read_le_u32()) != CDH_SIGNATURE {
            return invalid_signature();
        }

        h.version_made_by = try!(r.read_le_u16());
        h.version_needed_to_extract = try!(r.read_le_u16());
        h.general_purpose_bit_flag = try!(r.read_le_u16());
        h.compression_method = try!(r.read_le_u16());
        h.last_modified_datetime = try!(MsdosDateTime::read(r));
        h.crc32 = try!(r.read_le_u32());
        h.compressed_size = try!(r.read_le_u32());
        h.uncompressed_size = try!(r.read_le_u32());
        h.file_name_length = try!(r.read_le_u16());
        h.extra_field_length = try!(r.read_le_u16());
        h.file_comment_length = try!(r.read_le_u16());
        h.disk_number_start = try!(r.read_le_u16());
        h.internal_file_attributes = try!(r.read_le_u16());
        h.external_file_attributes = try!(r.read_le_u32());
        h.relative_offset_of_local_header = try!(r.read_le_u32());
        h.file_name = str::from_utf8_owned(try!(r.read_exact(h.file_name_length as uint))).unwrap();
        h.extra_field = try!(r.read_exact(h.extra_field_length as uint));
        h.file_comment = str::from_utf8_owned(try!(r.read_exact(h.file_comment_length as uint))).unwrap();

        // check for some things we don't support (yet?)
        // TODO

        Ok(h)
    }

    pub fn write<T:Writer>(&self, w: &mut T) -> IoResult<()> {
        try!(w.write_le_u32(CDH_SIGNATURE));
        try!(w.write_le_u16(self.version_made_by));
        try!(w.write_le_u16(self.version_needed_to_extract));
        try!(w.write_le_u16(self.general_purpose_bit_flag));
        try!(w.write_le_u16(self.compression_method));
        try!(self.last_modified_datetime.write(w));
        try!(w.write_le_u32(self.crc32));
        try!(w.write_le_u32(self.compressed_size));
        try!(w.write_le_u32(self.uncompressed_size));
        try!(w.write_le_u16(self.file_name_length));
        try!(w.write_le_u16(self.extra_field_length));
        try!(w.write_le_u16(self.file_comment_length));
        try!(w.write_le_u16(self.disk_number_start));
        try!(w.write_le_u16(self.internal_file_attributes));
        try!(w.write_le_u32(self.external_file_attributes));
        try!(w.write_le_u32(self.relative_offset_of_local_header));
        try!(w.write(self.file_name.as_bytes()));
        try!(w.write(self.extra_field.as_slice()));
        try!(w.write(self.file_comment.as_bytes()));
        Ok(())
    }
}

pub static CDDS_SIGNATURE: u32 = 0x05054b50;

pub struct CentralDirectoryDigitalSignature {
    pub data_size: u16,
    pub data: Vec<u8>
}


// ==== END OF CENTRAL DIRECTORY RECORD ====

pub static EOCDR_SIGNATURE: u32 = 0x06054b50;

pub struct EndOfCentralDirectoryRecord {
    pub disk_number: u16,
    pub disk_number_with_start_of_central_directory: u16,
    pub entry_count_this_disk: u16,
    pub total_entry_count: u16,
    pub central_directory_size: u32,
    pub central_directory_offset: u32,
    pub comment_length: u16,
    pub comment: String
}

impl EndOfCentralDirectoryRecord {
    pub fn new() -> EndOfCentralDirectoryRecord {
        EndOfCentralDirectoryRecord {
            disk_number: 0,
            disk_number_with_start_of_central_directory: 0,
            entry_count_this_disk: 0,
            total_entry_count: 0,
            central_directory_size: 0,
            central_directory_offset: 0,
            comment_length: 0,
            comment: String::new()
        }
    }

    pub fn read<T:Reader>(r: &mut T) -> IoResult<EndOfCentralDirectoryRecord> {
        let mut h = EndOfCentralDirectoryRecord::new();

        if try!(r.read_le_u32()) != EOCDR_SIGNATURE {
            return invalid_signature();
        }

        h.disk_number = try!(r.read_le_u16());
        h.disk_number_with_start_of_central_directory = try!(r.read_le_u16());
        h.entry_count_this_disk = try!(r.read_le_u16());
        h.total_entry_count = try!(r.read_le_u16());
        h.central_directory_size = try!(r.read_le_u32());
        h.central_directory_offset = try!(r.read_le_u32());
        h.comment_length = try!(r.read_le_u16());
        h.comment = str::from_utf8_owned(try!(r.read_exact(h.comment_length as uint))).unwrap();

        // check for some things we don't support (yet?)
        // TODO

        Ok(h)
    }

    pub fn write<T:Writer>(&self, w: &mut T) -> IoResult<()> {
        try!(w.write_le_u32(EOCDR_SIGNATURE));
        try!(w.write_le_u16(self.disk_number));
        try!(w.write_le_u16(self.disk_number_with_start_of_central_directory));
        try!(w.write_le_u16(self.entry_count_this_disk));
        try!(w.write_le_u16(self.total_entry_count));
        try!(w.write_le_u32(self.central_directory_size));
        try!(w.write_le_u32(self.central_directory_offset));
        try!(w.write_le_u16(self.comment_length));
        try!(w.write(self.comment.as_bytes()));
        Ok(())
    }

}

