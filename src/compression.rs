use anyhow::{bail, Context, Result};
use bzip2::write::BzEncoder;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, PartialEq, Copy, Clone, Default)]
pub enum CompressionFormat {
    Bzip2,
    Gzip,
    #[default]
    None,
    Xz,
    Zstd,
}

impl FromStr for CompressionFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "b" => Ok(CompressionFormat::Bzip2),
            "g" => Ok(CompressionFormat::Gzip),
            "x" => Ok(CompressionFormat::Xz),
            "z" => Ok(CompressionFormat::Zstd),
            "u" => Ok(CompressionFormat::None),
            _ => bail!("Invalid compression format: {}", s),
        }
    }
}

impl std::fmt::Display for CompressionFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format = match self {
            CompressionFormat::Bzip2 => "bz2",
            CompressionFormat::Gzip => "gz",
            CompressionFormat::None => "",
            CompressionFormat::Xz => "xz",
            CompressionFormat::Zstd => "zst",
        };
        write!(f, "{}", format)
    }
}

impl CompressionFormat {
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        detect_compression_format(reader)
    }

    /// Detect the compression format of a file based on its path extension.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let extension = path.extension().and_then(|s| s.to_str());

        match extension {
            Some("bz2") => Ok(CompressionFormat::Bzip2),
            Some("gz") => Ok(CompressionFormat::Gzip),
            Some("xz") => Ok(CompressionFormat::Xz),
            Some("zst") | Some("zstd") => Ok(CompressionFormat::Zstd),
            _ => Ok(CompressionFormat::None),
        }
    }

    pub fn is_compressed(&self) -> bool {
        *self != CompressionFormat::None
    }

    pub fn add_extension<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let mut path_buf = path.as_ref().to_path_buf();

        if !self.is_compressed() {
            return path_buf;
        }

        let new_extension = self.to_string();
        if let Some(existing_extension) = path_buf.extension() {
            let combined_extension =
                format!("{}.{}", existing_extension.to_string_lossy(), new_extension);
            path_buf.set_extension(combined_extension);
        } else {
            path_buf.set_extension(new_extension);
        }

        path_buf
    }

    pub fn compress<P: AsRef<Path>>(&self, input: P, output: P, threads: usize) -> Result<()> {
        let mut input_file = File::open(input)?;
        let mut output_file = File::create(output).context("Failed to create output file")?;

        let result = match self {
            Self::None => io::copy(&mut input_file, &mut output_file),
            Self::Bzip2 => bzip2_compress(&mut input_file, &mut output_file),
            Self::Gzip => gzip_compress(&mut input_file, &mut output_file, threads),
            Self::Xz => xz_compress(&mut input_file, &mut output_file, threads),
            Self::Zstd => zstd_compress(&mut input_file, &mut output_file, threads),
        };

        if let Err(e) = result {
            bail!("Failed to compress file: {}", e);
        }
        Ok(())
    }
}

fn bzip2_compress<R, W>(input: &mut R, output: &mut W) -> io::Result<u64>
where
    R: Read,
    W: Write,
{
    let mut encoder = BzEncoder::new(output, bzip2::Compression::default());
    let bytes = io::copy(input, &mut encoder)?;
    let _ = encoder.finish()?;
    Ok(bytes)
}

fn gzip_compress<R, W>(_input: &mut R, _output: &mut W, _threads: usize) -> io::Result<u64>
where
    R: Read,
    W: Write,
{
    unimplemented!()
}

fn xz_compress<R, W>(_input: &mut R, _output: &mut W, _threads: usize) -> io::Result<u64>
where
    R: Read,
    W: Write,
{
    unimplemented!()
}

fn zstd_compress<R, W>(input: &mut R, output: &mut W, threads: usize) -> io::Result<u64>
where
    R: Read,
    W: Write,
{
    let mut encoder = zstd::stream::write::Encoder::new(output, zstd::DEFAULT_COMPRESSION_LEVEL)?;
    encoder.multithread(threads as u32)?;

    let bytes = io::copy(input, &mut encoder)?;
    let _ = encoder.finish()?;
    Ok(bytes)
}

/// Detect the compression format of a file based on its magic number.
fn detect_compression_format<R: Read + Seek>(reader: &mut R) -> Result<CompressionFormat> {
    let original_position = reader.stream_position()?;

    // move the reader to the start of the file
    reader.seek(SeekFrom::Start(0))?;

    let mut magic = [0; 5];
    reader
        .read_exact(&mut magic)
        .context("Failed to read the first five bytes of the file")?;

    let format = match magic {
        [0x1f, 0x8b, ..] => CompressionFormat::Gzip,
        [0x42, 0x5a, ..] => CompressionFormat::Bzip2,
        [0x28, 0xb5, 0x2f, 0xfd, ..] => CompressionFormat::Zstd,
        [0xfd, 0x37, 0x7a, 0x58, 0x5a] => CompressionFormat::Xz,
        _ => CompressionFormat::None,
    };

    // Seek back to the original position
    reader
        .seek(SeekFrom::Start(original_position))
        .context("Failed to return reader to original position")?;

    Ok(format)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_detect_gzip_format() {
        let data = vec![
            0x1f, 0x8b, 0x08, 0x08, 0x1c, 0x6b, 0xe2, 0x66, 0x00, 0x03, 0x74, 0x65, 0x78, 0x74,
            0x2e, 0x74, 0x78, 0x74, 0x00, 0x4b, 0xcb, 0xcf, 0x57, 0x48, 0x4a, 0x2c, 0xe2, 0x02,
            0x00, 0x27, 0xb4, 0xdd, 0x13, 0x08, 0x00, 0x00, 0x00,
        ];
        let mut reader = Cursor::new(data);
        // position the reader at the original position
        let original_position = reader.position();
        let format = detect_compression_format(&mut reader).unwrap();
        assert_eq!(format, CompressionFormat::Gzip);
        assert_eq!(reader.position(), original_position);
    }

    #[test]
    fn test_detect_bzip2_format() {
        let data = vec![
            0x42, 0x5a, 0x68, 0x39, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x7b, 0x6e, 0xa8, 0x38,
            0x00, 0x00, 0x02, 0x51, 0x80, 0x00, 0x10, 0x40, 0x00, 0x31, 0x00, 0x90, 0x00, 0x20,
            0x00, 0x22, 0x1a, 0x63, 0x50, 0x86, 0x00, 0x2c, 0x8c, 0x3c, 0x5d, 0xc9, 0x14, 0xe1,
            0x42, 0x41, 0xed, 0xba, 0xa0, 0xe0,
        ];
        let mut reader = Cursor::new(data);
        // position the reader at the original position
        let original_position = reader.position();
        let format = detect_compression_format(&mut reader).unwrap();
        assert_eq!(format, CompressionFormat::Bzip2);
        assert_eq!(reader.position(), original_position);
    }

    #[test]
    fn test_detect_zstd_format() {
        let data = vec![
            0x28, 0xb5, 0x2f, 0xfd, 0x24, 0x08, 0x41, 0x00, 0x00, 0x66, 0x6f, 0x6f, 0x20, 0x62,
            0x61, 0x72, 0x0a, 0x37, 0x17, 0xa5, 0xec,
        ];
        let mut reader = Cursor::new(data);
        // position the reader at the original position
        let original_position = reader.position();
        let format = detect_compression_format(&mut reader).unwrap();
        assert_eq!(format, CompressionFormat::Zstd);
        assert_eq!(reader.position(), original_position);
    }

    #[test]
    fn test_detect_xz_format() {
        let data = vec![
            0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, 0x00, 0x04, 0xe6, 0xd6, 0xb4, 0x46, 0x02, 0x00,
            0x21, 0x01, 0x16, 0x00, 0x00, 0x00, 0x74, 0x2f, 0xe5, 0xa3, 0x01, 0x00, 0x07, 0x66,
            0x6f, 0x6f, 0x20, 0x62, 0x61, 0x72, 0x0a, 0x00, 0xfd, 0xbb, 0xfb, 0x3b, 0x8e, 0xcc,
            0x32, 0x13, 0x00, 0x01, 0x20, 0x08, 0xbb, 0x19, 0xd9, 0xbb, 0x1f, 0xb6, 0xf3, 0x7d,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x59, 0x5a,
        ];
        let mut reader = Cursor::new(data);
        // position the reader at the original position
        let original_position = reader.position();
        let format = detect_compression_format(&mut reader).unwrap();
        assert_eq!(format, CompressionFormat::Xz);

        // confirm that the reader is still at the original position
        assert_eq!(reader.position(), original_position);
    }

    #[test]
    fn test_detect_none_format() {
        let data = b"I'm not compressed";
        let mut reader = Cursor::new(data);
        let format = detect_compression_format(&mut reader).unwrap();
        assert_eq!(format, CompressionFormat::None);
    }

    #[test]
    fn test_detect_format_when_reader_is_part_way_through() {
        let data = vec![
            0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00, 0x00, 0x04, 0xe6, 0xd6, 0xb4, 0x46, 0x02, 0x00,
            0x21, 0x01, 0x16, 0x00, 0x00, 0x00, 0x74, 0x2f, 0xe5, 0xa3, 0x01, 0x00, 0x07, 0x66,
            0x6f, 0x6f, 0x20, 0x62, 0x61, 0x72, 0x0a, 0x00, 0xfd, 0xbb, 0xfb, 0x3b, 0x8e, 0xcc,
            0x32, 0x13, 0x00, 0x01, 0x20, 0x08, 0xbb, 0x19, 0xd9, 0xbb, 0x1f, 0xb6, 0xf3, 0x7d,
            0x01, 0x00, 0x00, 0x00, 0x00, 0x04, 0x59, 0x5a,
        ];
        let mut reader = Cursor::new(data);
        reader.seek(SeekFrom::Start(10)).unwrap();
        // position the reader at the original position
        let original_position = reader.position();
        let format = detect_compression_format(&mut reader).unwrap();
        assert_eq!(format, CompressionFormat::Xz);

        // confirm that the reader is still at the original position
        assert_eq!(reader.position(), original_position);
    }

    #[test]
    fn test_compression_format_from_str() {
        let format = "b".parse::<CompressionFormat>().unwrap();
        assert_eq!(format, CompressionFormat::Bzip2);

        let format = "g".parse::<CompressionFormat>().unwrap();
        assert_eq!(format, CompressionFormat::Gzip);

        let format = "x".parse::<CompressionFormat>().unwrap();
        assert_eq!(format, CompressionFormat::Xz);

        let format = "z".parse::<CompressionFormat>().unwrap();
        assert_eq!(format, CompressionFormat::Zstd);

        let format = "Z".parse::<CompressionFormat>().unwrap();
        assert_eq!(format, CompressionFormat::Zstd);

        let format = "u".parse::<CompressionFormat>().unwrap();
        assert_eq!(format, CompressionFormat::None);

        let format = "J".parse::<CompressionFormat>();
        assert!(format.is_err());
    }

    #[test]
    fn test_compression_format_from_path() {
        let format = CompressionFormat::from_path("file.txt").unwrap();
        assert_eq!(format, CompressionFormat::None);

        let format = CompressionFormat::from_path("file.txt.gz").unwrap();
        assert_eq!(format, CompressionFormat::Gzip);

        let format = CompressionFormat::from_path("file.txt.bz2").unwrap();
        assert_eq!(format, CompressionFormat::Bzip2);

        let format = CompressionFormat::from_path("file.txt.xz").unwrap();
        assert_eq!(format, CompressionFormat::Xz);

        let format = CompressionFormat::from_path("file.txt.zst").unwrap();
        assert_eq!(format, CompressionFormat::Zstd);

        let format = CompressionFormat::from_path("file.txt.zstd").unwrap();
        assert_eq!(format, CompressionFormat::Zstd);
    }

    #[test]
    fn test_compression_format_display() {
        let format = CompressionFormat::Bzip2;
        assert_eq!(format.to_string(), "bz2");

        let format = CompressionFormat::Gzip;
        assert_eq!(format.to_string(), "gz");

        let format = CompressionFormat::None;
        assert_eq!(format.to_string(), "");

        let format = CompressionFormat::Xz;
        assert_eq!(format.to_string(), "xz");

        let format = CompressionFormat::Zstd;
        assert_eq!(format.to_string(), "zst");
    }

    #[test]
    fn test_compression_format_is_compressed() {
        let format = CompressionFormat::Bzip2;
        assert!(format.is_compressed());

        let format = CompressionFormat::Gzip;
        assert!(format.is_compressed());

        let format = CompressionFormat::None;
        assert!(!format.is_compressed());

        let format = CompressionFormat::Xz;
        assert!(format.is_compressed());

        let format = CompressionFormat::Zstd;
        assert!(format.is_compressed());
    }

    #[test]
    fn test_compression_format_add_extension() {
        let format = CompressionFormat::Bzip2;
        let path = Path::new("file.txt");
        let new_path = format.add_extension(path);
        assert_eq!(new_path, PathBuf::from("file.txt.bz2"));

        let format = CompressionFormat::Gzip;
        let path = Path::new("file.txt");
        let new_path = format.add_extension(path);
        assert_eq!(new_path, PathBuf::from("file.txt.gz"));

        let format = CompressionFormat::None;
        let path = Path::new("file.txt");
        let new_path = format.add_extension(path);
        assert_eq!(new_path, PathBuf::from("file.txt"));

        let format = CompressionFormat::Xz;
        let path = Path::new("file.txt");
        let new_path = format.add_extension(path);
        assert_eq!(new_path, PathBuf::from("file.txt.xz"));

        let format = CompressionFormat::Zstd;
        let path = Path::new("file.txt");
        let new_path = format.add_extension(path);
        assert_eq!(new_path, PathBuf::from("file.txt.zst"));
    }

    #[test]
    fn test_bzip2_compress() {
        let data = b"foo bar\n";
        let mut reader = Cursor::new(data);
        let mut writer = Cursor::new(Vec::new());
        let bytes = bzip2_compress(&mut reader, &mut writer).unwrap();
        let expected = vec![
            0x42, 0x5a, 0x68, 0x36, 0x31, 0x41, 0x59, 0x26, 0x53, 0x59, 0x7b, 0x6e, 0xa8, 0x38,
            0x00, 0x00, 0x02, 0x51, 0x80, 0x00, 0x10, 0x40, 0x00, 0x31, 0x00, 0x90, 0x00, 0x20,
            0x00, 0x22, 0x1a, 0x63, 0x50, 0x86, 0x00, 0x2c, 0x8c, 0x3c, 0x5d, 0xc9, 0x14, 0xe1,
            0x42, 0x41, 0xed, 0xba, 0xa0, 0xe0,
        ];
        assert_eq!(bytes, data.len() as u64);
        assert_eq!(writer.into_inner(), expected);
    }

    #[test]
    fn test_zstd_compress() {
        let data = b"foo bar\n";
        let mut reader = Cursor::new(data);
        let mut writer = Cursor::new(Vec::new());
        let bytes = zstd_compress(&mut reader, &mut writer, 4).unwrap();
        let expected = vec![
            0x28, 0xb5, 0x2f, 0xfd, 0x24, 0x08, 0x41, 0x00, 0x00, 0x66, 0x6f, 0x6f, 0x20, 0x62,
            0x61, 0x72, 0x0a, 0x37, 0x17, 0xa5, 0xec,
        ];
        assert_eq!(bytes, data.len() as u64);
        assert_eq!(writer.into_inner(), expected);
    }
}
