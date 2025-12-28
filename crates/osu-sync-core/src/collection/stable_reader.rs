//! Parser for osu!stable's collection.db binary format
//!
//! The collection.db file uses the following binary format:
//! - i32: Version number (e.g., 20150203)
//! - i32: Number of collections
//! - For each collection:
//!   - String: Collection name (0x0b marker, ULEB128 length, UTF-8 bytes)
//!   - i32: Number of beatmaps
//!   - For each beatmap: String (MD5 hash in same format)

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use super::Collection;
use crate::error::{Error, Result};

/// Reader for osu!stable collection.db files
pub struct StableCollectionReader;

impl StableCollectionReader {
    /// Read collections from osu!stable's collection.db file
    ///
    /// Returns an empty vector if the file doesn't exist or is empty.
    /// Returns an error for other I/O or parse errors.
    pub fn read<P: AsRef<Path>>(path: P) -> Result<Vec<Collection>> {
        let path = path.as_ref();

        // Return empty vec if file doesn't exist
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        Self::parse(&mut reader)
    }

    /// Parse the collection.db binary format from a reader
    fn parse<R: Read>(reader: &mut R) -> Result<Vec<Collection>> {
        // Read version number (i32, little-endian)
        let _version = Self::read_i32(reader)?;

        // Read collection count
        let count = Self::read_i32(reader)?;
        if count < 0 {
            return Err(Error::Other("Invalid collection count".to_string()));
        }

        let mut collections = Vec::with_capacity(count as usize);

        for _ in 0..count {
            // Read collection name
            let name =
                Self::read_string(reader)?.unwrap_or_else(|| "Unnamed Collection".to_string());

            // Read beatmap count for this collection
            let beatmap_count = Self::read_i32(reader)?;
            if beatmap_count < 0 {
                return Err(Error::Other(
                    "Invalid beatmap count in collection".to_string(),
                ));
            }

            // Read beatmap hashes
            let mut hashes = Vec::with_capacity(beatmap_count as usize);
            for _ in 0..beatmap_count {
                if let Some(hash) = Self::read_string(reader)? {
                    if !hash.is_empty() {
                        hashes.push(hash);
                    }
                }
            }

            collections.push(Collection::with_hashes(name, hashes));
        }

        Ok(collections)
    }

    /// Read a little-endian i32
    fn read_i32<R: Read>(reader: &mut R) -> Result<i32> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(i32::from_le_bytes(buf))
    }

    /// Read an osu! format string
    ///
    /// Format:
    /// - 0x00: null/empty string
    /// - 0x0b: string follows (ULEB128 length, then UTF-8 bytes)
    fn read_string<R: Read>(reader: &mut R) -> Result<Option<String>> {
        let mut marker = [0u8; 1];
        reader.read_exact(&mut marker)?;

        match marker[0] {
            0x00 => Ok(None),
            0x0b => {
                let length = Self::read_uleb128(reader)?;
                if length == 0 {
                    return Ok(Some(String::new()));
                }

                let mut buf = vec![0u8; length as usize];
                reader.read_exact(&mut buf)?;

                String::from_utf8(buf)
                    .map(Some)
                    .map_err(|e| Error::Other(format!("Invalid UTF-8 in string: {}", e)))
            }
            other => {
                // Some older formats might have different markers
                Err(Error::Other(format!(
                    "Unknown string marker: 0x{:02x}",
                    other
                )))
            }
        }
    }

    /// Read a ULEB128 (unsigned LEB128) encoded integer
    ///
    /// ULEB128 uses 7 bits per byte for data, with the high bit as a continuation flag.
    fn read_uleb128<R: Read>(reader: &mut R) -> Result<u32> {
        let mut result: u32 = 0;
        let mut shift = 0;

        loop {
            let mut byte = [0u8; 1];
            reader.read_exact(&mut byte)?;
            let byte = byte[0];

            // Extract the 7 data bits
            result |= ((byte & 0x7F) as u32) << shift;

            // Check continuation bit
            if byte & 0x80 == 0 {
                break;
            }

            shift += 7;
            if shift >= 35 {
                return Err(Error::Other("ULEB128 value too large".to_string()));
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn write_i32(buf: &mut Vec<u8>, value: i32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn write_uleb128(buf: &mut Vec<u8>, mut value: u32) {
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            buf.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    fn write_string(buf: &mut Vec<u8>, s: &str) {
        if s.is_empty() {
            buf.push(0x00);
        } else {
            buf.push(0x0b);
            write_uleb128(buf, s.len() as u32);
            buf.extend_from_slice(s.as_bytes());
        }
    }

    #[test]
    fn test_parse_empty_db() {
        let mut data = Vec::new();
        write_i32(&mut data, 20150203); // version
        write_i32(&mut data, 0); // 0 collections

        let mut cursor = Cursor::new(data);
        let collections = StableCollectionReader::parse(&mut cursor).unwrap();
        assert!(collections.is_empty());
    }

    #[test]
    fn test_parse_single_collection() {
        let mut data = Vec::new();
        write_i32(&mut data, 20150203); // version
        write_i32(&mut data, 1); // 1 collection
        write_string(&mut data, "My Collection"); // collection name
        write_i32(&mut data, 2); // 2 beatmaps
        write_string(&mut data, "d41d8cd98f00b204e9800998ecf8427e"); // hash 1
        write_string(&mut data, "098f6bcd4621d373cade4e832627b4f6"); // hash 2

        let mut cursor = Cursor::new(data);
        let collections = StableCollectionReader::parse(&mut cursor).unwrap();

        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].name, "My Collection");
        assert_eq!(collections[0].beatmap_hashes.len(), 2);
        assert_eq!(
            collections[0].beatmap_hashes[0],
            "d41d8cd98f00b204e9800998ecf8427e"
        );
        assert_eq!(
            collections[0].beatmap_hashes[1],
            "098f6bcd4621d373cade4e832627b4f6"
        );
    }

    #[test]
    fn test_parse_multiple_collections() {
        let mut data = Vec::new();
        write_i32(&mut data, 20150203); // version
        write_i32(&mut data, 2); // 2 collections

        // Collection 1
        write_string(&mut data, "Favorites");
        write_i32(&mut data, 1);
        write_string(&mut data, "abc123");

        // Collection 2
        write_string(&mut data, "Training");
        write_i32(&mut data, 0); // empty collection

        let mut cursor = Cursor::new(data);
        let collections = StableCollectionReader::parse(&mut cursor).unwrap();

        assert_eq!(collections.len(), 2);
        assert_eq!(collections[0].name, "Favorites");
        assert_eq!(collections[0].beatmap_hashes.len(), 1);
        assert_eq!(collections[1].name, "Training");
        assert!(collections[1].beatmap_hashes.is_empty());
    }

    #[test]
    fn test_uleb128_small() {
        // Test small values (single byte)
        let data = vec![127u8]; // 127
        let mut cursor = Cursor::new(data);
        assert_eq!(
            StableCollectionReader::read_uleb128(&mut cursor).unwrap(),
            127
        );
    }

    #[test]
    fn test_uleb128_multi_byte() {
        // Test multi-byte value: 300 = 0b100101100
        // ULEB128: 0xAC 0x02 (172, 2)
        let data = vec![0xAC, 0x02];
        let mut cursor = Cursor::new(data);
        assert_eq!(
            StableCollectionReader::read_uleb128(&mut cursor).unwrap(),
            300
        );
    }
}
