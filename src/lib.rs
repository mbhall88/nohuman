pub mod download;

use serde::Deserialize;
use std::ffi::OsStr;
use std::io::{self, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs::File;
use serde::Serialize;
use anyhow::{Context, Result};
use serde_json;
use rayon::prelude::*;

use niffler::{get_writer, compression, from_path, error::Error as NifflerError};
use gzp::{deflate::Gzip, ZBuilder};
use gzp::deflate::Bgzf;
use zstd::stream::Encoder as ZstdEncoder;
use liblzma::write::XzEncoder;

/// Wrapper function to simplify running of different compression functions
pub fn write_output(
    tmpout1: &PathBuf,
    tmpout2: Option<&PathBuf>, // Optional second output file
    out1: &PathBuf,
    out2: Option<&PathBuf>,    // Optional second output file
    compression_threads: usize // The number of threads for compression
) -> Result<(), anyhow::Error> {
    let mut niffler_pairs = Vec::new(); // Store niffler compression pairs

    // Handle out1 based on its compression type
    let compression_type1 = determine_compression_type(out1);
    match compression_type1.as_str() {
        "gz" | "bgz" => write_with_gzp(tmpout1, out1, compression_threads)?,
        "zst" | "zstd" => write_with_zstd(tmpout1, out1, compression_threads)?,
        "xz" | "lzma" => write_with_liblzma(tmpout1, out1, compression_threads, 6)?,
        "no" | "bz2" => {
            // Collect niffler pairs for out1
            niffler_pairs.push((vec![tmpout1.clone()], vec![out1.clone()]));
        }
        _ => return Err(anyhow::anyhow!("Unsupported compression type: {}", compression_type1)),
    }

    // Handle out2 if it exists and is different from out1
    if let Some(tmp2) = tmpout2 {
        if let Some(out2) = out2 {
            let compression_type2 = determine_compression_type(out2);
            match compression_type2.as_str() {
                "gz" | "bgz" => write_with_gzp(tmp2, out2, compression_threads)?,
                "zst" | "zstd" => write_with_zstd(tmp2, out2, compression_threads)?,
                "xz" | "lzma" => write_with_liblzma(tmp2, out2, compression_threads, 6)?,
                "no" | "bz2" => {
                    // Collect niffler pairs for out2
                    niffler_pairs.push((vec![tmp2.clone()], vec![out2.clone()]));
                }
                _ => return Err(anyhow::anyhow!("Unsupported compression type: {}", compression_type2)),
            }
        }
    }

    // Process niffler pairs in parallel (for no or bgz compression)
    // These formats are handled differently since parallel compression
    // of a single file is not possible, so
    if !niffler_pairs.is_empty() {
        niffler_pairs.into_par_iter().try_for_each(|(tmpout, out)| {
            write_with_niffler(tmpout, out, 1) // Always use 1 thread per file
        })?;
    }

    Ok(())
}

pub fn determine_compression_type(output_path: &PathBuf) -> String {
    match output_path.extension().unwrap_or_default().to_str().unwrap_or_default() {
        "gz" => "gz".to_string(),
        "xz" => "xz".to_string(),
        "lzma" => "lzma".to_string(),
        "zst" | "zstd" => "zst".to_string(),
        "bz2" => "bz2".to_string(),
        "bgz" => "bgz".to_string(),
        _ => "no".to_string(), // Default to no compression
    }
}

/// Function to write and compress using XZ with configurable threads
pub fn write_with_liblzma(input_path: &PathBuf, output_path: &PathBuf, threads: usize, level: u32) -> io::Result<()> {
    // Open the input file
    let input_file = File::open(input_path)?;
    let mut reader = BufReader::new(input_file);

    // Create the output file with a `.xz` extension
    let output_file = File::create(output_path)?;
    let writer = BufWriter::new(output_file);

    // Choose the encoder based on the number of threads
    let mut encoder = if threads > 1 {
        // Use multithreaded encoder
        XzEncoder::new_parallel(writer, level)  // Parallel compression with the specified level
    } else {
        // Use single-threaded encoder
        XzEncoder::new(writer, level)  // Single-thread compression with the specified level
    };

    // Compress the input file data
    io::copy(&mut reader, &mut encoder)?;

    // Finalize the compression process
    encoder.finish()?;

    Ok(())
}

/// Utility function to gzip or BGZF output files using the gzp crate
pub fn write_with_gzp(input_path: &PathBuf, output_path: &PathBuf, threads: usize) -> io::Result<()> {
    // Open the input file
    let input_file = File::open(input_path)?;
    let mut reader = BufReader::new(input_file);

    // Create the output file based on its extension
    let extension = output_path.extension().unwrap_or_default().to_str().unwrap_or_default();
    let output_file = File::create(output_path)?;

    let writer = BufWriter::new(output_file);

    // Configure the compressor with the specified number of threads and compression format
    let mut compressor = match extension {
        "gz" => ZBuilder::<Gzip, _>::new()
            .num_threads(threads)
            .from_writer(writer), // Use Gzip compression for .gz files
        "bgz" => ZBuilder::<Bgzf, _>::new()
            .num_threads(threads)
            .from_writer(writer), // Use BGZF compression for .bgz files
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "Unsupported file extension")),
    };

    // Compress the input file data
    io::copy(&mut reader, &mut compressor)?;

    // Finalize the compression process and map the error to io::Error
    compressor.finish().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
}

/// Utility function to compress files using the zstd crate with configurable multithreading
pub fn write_with_zstd(input_path: &PathBuf, output_path: &PathBuf, threads: usize) -> io::Result<()> {
    // Open the input file
    let input_file = File::open(input_path)?;
    let mut reader = BufReader::new(input_file);

    // Create the output file with a `.zst` extension
    let output_file = File::create(output_path)?;
    let writer = BufWriter::new(output_file);

    // Configure the compressor based on the number of threads
    let mut encoder = if threads > 1 {
        let mut encoder = ZstdEncoder::new(writer, 0)?;
        encoder.multithread(threads as u32)?; // Enable multithreading
        encoder
    } else {
        ZstdEncoder::new(writer, 0)? // Single-threaded mode
    };

    // Compress the input file data
    io::copy(&mut reader, &mut encoder)?;

    // Finalize the compression process
    encoder.finish()?;

    Ok(())
}

/// Compress a file using niffler with dynamic format detection based on the file extension
pub fn write_with_niffler(input_paths: Vec<PathBuf>, output_paths: Vec<PathBuf>, threads: usize) -> Result<(), NifflerError> {
    // Set the number of threads for parallelism using a local thread pool
    rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap().install(|| -> Result<(), NifflerError> {

        // Parallelize the compression of multiple files
        input_paths.into_par_iter().zip(output_paths.into_par_iter()).try_for_each(|(input_path, output_path)| -> Result<(), NifflerError> {
            // Open the input file and detect its compression format
            let input_file = File::open(&input_path).map_err(NifflerError::IOError)?;
            let mut reader = BufReader::new(input_file);

            // Create the output file with appropriate compression format based on the extension
            // In this program we are only using niffler for bgz and no compression, but the others
            // are included here for completeness.
            let extension = output_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            let format = match extension {
                "gz" => compression::Format::Gzip,
                "bz2" => compression::Format::Bzip,
                "xz" => compression::Format::Lzma,
                "lzma" => compression::Format::Lzma,
                "zst" => compression::Format::Zstd,
                "zstd" => compression::Format::Zstd,
                _ => compression::Format::No,
            };

            // Use niffler to create the output file
            let output_file = File::create(&output_path).map_err(NifflerError::IOError)?;
            let writer = BufWriter::new(output_file);
            let mut compressor = get_writer(Box::new(writer), format, niffler::Level::One)?;

            // Compress the input file data
            io::copy(&mut reader, &mut compressor).map_err(NifflerError::IOError)?;

            // Finalize the compression
            compressor.flush().map_err(NifflerError::IOError)?;
            Ok(())
        })
    })?;
    
    Ok(())
}

pub fn read_with_niffler(input_paths: Vec<PathBuf>, output_paths: Vec<PathBuf>, compression_threads: usize) -> Result<(), NifflerError> {
    if compression_threads > 1 {
        // Decompress both files in parallel, with each file using a single thread
        rayon::ThreadPoolBuilder::new()
            .num_threads(2) // Create a thread pool with exactly 2 threads for parallel processing
            .build()
            .unwrap()
            .install(|| -> Result<(), NifflerError> {
                input_paths.into_par_iter().zip(output_paths.into_par_iter()).try_for_each(|(input_path, output_path)| {
                    // Each decompression task is run in parallel using a single thread
                    rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap().install(|| {
                        let (mut reader, _format) = from_path(&input_path)?;
                        let output_file = File::create(&output_path).map_err(NifflerError::IOError)?;
                        let mut writer = BufWriter::new(output_file);
                        io::copy(&mut reader, &mut writer).map_err(NifflerError::IOError)?;
                        writer.flush().map_err(NifflerError::IOError)?;
                        Ok(())
                    })
                })
            })?;
    } else {
        // Sequential decompression without any thread pool
        for (input_path, output_path) in input_paths.into_iter().zip(output_paths.into_iter()) {
            let (mut reader, _format) = from_path(&input_path)?;
            let output_file = File::create(&output_path).map_err(NifflerError::IOError)?;
            let mut writer = BufWriter::new(output_file);
            io::copy(&mut reader, &mut writer).map_err(NifflerError::IOError)?;
            writer.flush().map_err(NifflerError::IOError)?;
        }
    }

    Ok(())
}

/// Parse Kraken2 stderr for stats
pub fn parse_kraken_stats(kraken_stderr: &str) -> Result<Stats, anyhow::Error> {
    let mut total_sequences: usize = 0;
    let mut classified_sequences: usize = 0;
    let mut unclassified_sequences: usize = 0;

    // Parse Kraken2 stderr output line by line
    for line in kraken_stderr.lines() {
        if line.contains("processed") {
            total_sequences = line.split_whitespace()
                .nth(0)
                .unwrap()
                .replace(",", "") // Handle commas in large numbers
                .parse::<usize>()
                .expect("Failed to parse total sequences");
        } else if line.contains("sequences classified") {
            classified_sequences = line.split_whitespace()
                .nth(0)
                .unwrap()
                .replace(",", "") // Handle commas in large numbers
                .parse::<usize>()
                .expect("Failed to parse classified sequences");
        } else if line.contains("sequences unclassified") {
            unclassified_sequences = line.split_whitespace()
                .nth(0)
                .unwrap()
                .replace(",", "") // Handle commas in large numbers
                .parse::<usize>()
                .expect("Failed to parse unclassified sequences");
        }
    }

    let sequences_removed = classified_sequences;
    let sequences_remaining = unclassified_sequences;
    let proportion_removed = sequences_removed as f64 / total_sequences as f64;

    // Return stats
    Ok(Stats {
        nohuman_version: env!("CARGO_PKG_VERSION").to_string(),
        kraken2_version: "".to_string(),  // Placeholder, to be filled later
        input1: "".to_string(),  // Placeholder, to be filled later
        input2: "".to_string(),  // Placeholder, to be filled later
        output1: "".to_string(), // Placeholder, to be filled later
        output2: "".to_string(), // Placeholder, to be filled later
        total_sequences,
        sequences_removed,
        sequences_remaining,
        proportion_removed,
    })
}

/// Struct for JSON statistics output
#[derive(Serialize)]
pub struct Stats {
    pub nohuman_version: String,
    pub kraken2_version: String,
    pub input1: String,
    pub input2: String,
    pub output1: String,
    pub output2: String,
    pub total_sequences: usize,
    pub sequences_remaining: usize,
    pub sequences_removed: usize,
    pub proportion_removed: f64,
}

/// Write stats to a JSON file
pub fn write_stats(stats_file: &PathBuf, stats: &Stats) -> Result<(), anyhow::Error> {
    let json_data = serde_json::to_string_pretty(&stats)?;
    std::fs::write(stats_file, format!("{}\n", json_data))
        .context("Failed to write stats to file")?;
    Ok(())
}

#[derive(Deserialize)]
pub struct Config {
    pub database_url: String,
    pub database_md5: String,
}


impl Config {
    pub fn new(database_url: &str, database_md5: &str) -> Self {
        Self {
            database_url: database_url.to_string(),
            database_md5: database_md5.to_string(),
        }
    }
}

pub struct CommandRunner {
    pub command: String,
}

impl CommandRunner {
    pub fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
        }
    }

    pub fn run(&self, args: &[&str]) -> io::Result<()> {
        let output = Command::new(&self.command).args(args).output()?;

        if !output.status.success() {
            let error_message = String::from_utf8_lossy(&output.stderr);
            writeln!(io::stderr(), "{}", error_message)?;
        }

        Ok(())
    }

    pub fn is_executable(&self) -> bool {
        let cmd = format!("command -v {}", &self.command);
        let result = Command::new("sh").args(["-c", &cmd]).output();
        match result {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }
}

/// A utility function that allows the CLI to error if a path doesn't exist
pub fn check_path_exists<S: AsRef<OsStr> + ?Sized>(s: &S) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("{:?} does not exist", path))
    }
}

/// Checks if the specified path is a directory and contains the required kraken2 db files.
/// If not found, checks inside a 'db' subdirectory.
///
/// # Arguments
///
/// * `path` - A path to check for the required kraken2 db files.
///
/// # Returns
///
/// * `Result<PathBuf, String>` - Ok with the valid path if the files are found, Err otherwise.
pub fn validate_db_directory(path: &Path) -> Result<PathBuf, String> {
    let required_files = ["hash.k2d", "opts.k2d", "taxo.k2d"];
    let files_str = required_files.join(", ");

    // Check if the path is a directory and contains the required files
    if path.is_dir() && required_files.iter().all(|file| path.join(file).exists()) {
        return Ok(path.to_path_buf());
    }

    // Check inside a 'db' subdirectory
    let db_path = path.join("db");
    if db_path.is_dir()
        && required_files
            .iter()
            .all(|file| db_path.join(file).exists())
    {
        return Ok(db_path);
    }

    Err(format!(
        "Required files ({}) not found in {:?} or its 'db' subdirectory",
        files_str, path
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let command = CommandRunner::new("ls");
        assert_eq!(command.command, "ls");
    }

    #[test]
    fn test_run() {
        let command = CommandRunner::new("ls");
        let result = command.run(&["-l"]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_with_invalid_command() {
        let command = CommandRunner::new("not-a-real-command");
        let result = command.run(&["-l"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_executable() {
        let command = CommandRunner::new("ls");
        assert!(command.is_executable());
    }

    #[test]
    fn test_is_not_executable() {
        let command = CommandRunner::new("not-a-real-command");
        assert!(!command.is_executable());
    }

    #[test]
    fn check_path_exists_it_doesnt() {
        let result = check_path_exists(OsStr::new("fake.path"));
        assert!(result.is_err())
    }

    #[test]
    fn check_path_it_does() {
        let actual = check_path_exists(OsStr::new("Cargo.toml")).unwrap();
        let expected = PathBuf::from("Cargo.toml");
        assert_eq!(actual, expected)
    }

    // /// Function to write and compress using parallel XZ with multiple threads
    // /// Note: removed as using liblzma instead
    // pub fn write_with_xz2(input_path: &PathBuf, output_path: &PathBuf, threads: usize) -> Result<()> {
    //     // Open the input file
    //     let input_file = File::open(input_path)?;
    //     let mut reader = BufReader::new(input_file);

    //     // Create the output file with a `.xz` extension
    //     let output_file = File::create(output_path.with_extension("xz"))?;
    //     let writer = BufWriter::new(output_file);

    //     // Set up LZMA options for compression
    //     let mut lzma_options = LzmaOptions::new_preset(6)?;  // Use default preset level (6)
    //     // lzma_options.dict_size(64 * 1024 * 1024);            // Set dictionary size
    //     // lzma_options.match_finder(xz2::stream::MatchFinder::BinaryTree4);  // Best compression ratio

    //     // Build the XZ filters
    //     let mut filters = Filters::new();
    //     filters.lzma2(&lzma_options);

    //     // Build a multithreaded XZ stream
    //     let mut mt_stream = MtStreamBuilder::new()
    //         .threads(threads as u32)   // Set the number of threads
    //         .filters(filters)          // Apply LZMA filters
    //         .check(Check::Crc32)       // Set an integrity check (CRC32 here)
    //         .encoder()?;               // Initialize the encoder

    //     // Wrap the encoder with BufWriter for efficient writing
    //     let mut encoder = XzEncoder::new_stream(writer, mt_stream);

    //     // Compress the input file data
    //     io::copy(&mut reader, &mut encoder)?;

    //     // Finalize the compression process
    //     encoder.finish()?;

    //     Ok(())
    // }

    // /// Function to read and decompress using liblzma
    // pub fn read_with_liblzma(input_path: &PathBuf, output_path: &PathBuf) -> io::Result<()> {
    //     // Open the input file and create an XzDecoder
    //     let input_file = File::open(input_path)?;
    //     let mut reader = BufReader::new(XzDecoder::new(input_file)); 

    //     // Write the decompressed output to a new file
    //     let output_file = File::create(output_path)?;
    //     let mut writer = BufWriter::new(output_file);

    //     // Copy the decompressed data to the output file
    //     io::copy(&mut reader, &mut writer)?;
    //     writer.flush()?;

    //     Ok(())
    // }

}
