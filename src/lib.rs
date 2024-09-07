pub mod download;

use serde::Deserialize;
use std::ffi::OsStr;
use std::io::{self, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::Command;
use gzp::{deflate::Gzip, ZBuilder};
use std::fs::File;
use serde::Serialize;
use anyhow::{Context, Result};
use serde_json;

use niffler::{get_writer, compression, from_path, error::Error as NifflerError};
use rayon::prelude::*;

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
            let extension = output_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            let format = match extension {
                "gz" => compression::Format::Gzip,
                "bz2" => compression::Format::Bzip,
                "xz" => compression::Format::Lzma,
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

/// Decompress a file using niffler
pub fn read_with_niffler(input_paths: Vec<PathBuf>, output_paths: Vec<PathBuf>, threads: usize) -> Result<(), NifflerError> {
    // Set the number of threads for parallelism using a local thread pool
    rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap().install(|| -> Result<(), NifflerError> {

        input_paths.into_par_iter().zip(output_paths.into_par_iter()).try_for_each(|(input_path, output_path)| -> Result<(), NifflerError> {
            // Open the input file and detect its compression format
            let (mut reader, _format) = from_path(&input_path)?;

            // Write the decompressed output to a new file
            let output_file = File::create(&output_path).map_err(NifflerError::IOError)?;
            let mut writer = BufWriter::new(output_file);

            io::copy(&mut reader, &mut writer).map_err(NifflerError::IOError)?;
            writer.flush().map_err(NifflerError::IOError)?;
            Ok(())
        })
    })?;

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

/// Utility function to gzip output files using the gzp crate
pub fn compress_output(input_path: &PathBuf, output_path: &PathBuf, threads: usize) -> io::Result<()> {
    // Open the input file
    let input_file = File::open(input_path)?;
    let mut reader = BufReader::new(input_file);

    // Create the output file with a `.gz` extension
    let output_file = File::create(output_path.with_extension("gz"))?;
    let writer = BufWriter::new(output_file);

    // Configure the compressor with the specified number of threads (0 for single-threaded)
    let mut compressor = ZBuilder::<Gzip, _>::new()
        .num_threads(threads)
        .from_writer(writer);

    // Compress the input file data
    io::copy(&mut reader, &mut compressor)?;

    // Finalize the compression process and map the error to io::Error
    compressor.finish().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    Ok(())
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
}
