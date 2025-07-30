pub mod compression;
pub mod download;

use log::{debug, info};
use serde::Deserialize;
use std::ffi::OsStr;
use std::io::{self};
use std::num::ParseIntError;
use std::path::{Path, PathBuf};
use std::process::Command;

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

        let stderr_log = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Err(io::Error::other(format!(
                "{} failed with stderr {}",
                self.command, stderr_log
            )));
        }

        debug!("kraken2 stderr:\n {}", stderr_log);

        let (total, classified, unclassified) =
            parse_kraken_stderr(&stderr_log).unwrap_or((0, 0, 0));

        info!(
            "{} / {} ({:.2}%) sequences classified as human; {} ({:.2}%) as non-human",
            classified,
            total,
            (classified as f64 / total as f64) * 100.0,
            unclassified,
            (unclassified as f64 / total as f64) * 100.0
        );

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

/// Parses the kraken2 stderr to get thenumber of total, classified and unclassifed reads.
fn parse_kraken_stderr(stderr: &str) -> Result<(usize, usize, usize), ParseIntError> {
    let mut total_sequences: usize = 0;
    let mut classified_sequences: usize = 0;
    let mut unclassified_sequences: usize = 0;

    // Parse Kraken2 stderr output line by line
    for line in stderr.lines() {
        if line.contains("processed") {
            total_sequences = line
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .replace(",", "") // Handle commas in large numbers
                .parse::<usize>()?;
        } else if line.contains("sequences classified") {
            classified_sequences = line
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .replace(",", "") // Handle commas in large numbers
                .parse::<usize>()?;
        } else if line.contains("sequences unclassified") {
            unclassified_sequences = line
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .replace(",", "") // Handle commas in large numbers
                .parse::<usize>()?;
        }
    }

    Ok((
        total_sequences,
        classified_sequences,
        unclassified_sequences,
    ))
}

/// A utility function that allows the CLI to error if a path doesn't exist
pub fn check_path_exists<S: AsRef<OsStr> + ?Sized>(s: &S) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if path.exists() {
        Ok(path)
    } else {
        Err(format!("{path:?} does not exist",))
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
        "Required files ({files_str}) not found in {path:?} or its 'db' subdirectory",
    ))
}

/// Parse confidence score from the command line. Will be passed on to kraken2. Must be in the
/// closed interval [0, 1] - i.e. 0 <= confidence <= 1.
pub fn parse_confidence_score(s: &str) -> Result<f32, String> {
    let confidence: f32 = s.parse().map_err(|_| "Confidence score must be a number")?;
    if !(0.0..=1.0).contains(&confidence) {
        return Err("Confidence score must be in the closed interval [0, 1]".to_string());
    }
    Ok(confidence)
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

    #[test]
    fn test_parse_confidence_score() {
        let result = parse_confidence_score("0.5");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0.5);

        let result = parse_confidence_score("1.0");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1.0);

        let result = parse_confidence_score("0.0");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0.0);

        let result = parse_confidence_score("1.1");
        assert!(result.is_err());

        let result = parse_confidence_score("-0.1");
        assert!(result.is_err());
    }
}
