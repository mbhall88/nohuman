use flate2::read::GzDecoder;
use reqwest::blocking::get;
use std::fs;
use std::io::Read;
use std::path::Path;
use tar::Archive;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum DownloadError {
    #[error("Failed to download the tarball")]
    DownloadFailed,

    #[error("Failed to extract the tarball")]
    ExtractionFailed,

    // Add more error variants as needed
}

fn download_and_extract_tarball(
    url: &str,
    output_path: &str,
) -> Result<(), DownloadError> {
    // Create a temporary file to store the downloaded tarball
    let mut response = get(url).map_err(|_| DownloadError::DownloadFailed)?;
    let mut tarball = Vec::new();
    response.read_to_end(&mut tarball).map_err(|_| DownloadError::DownloadFailed)?;

    // Extract the tarball to the output path
    let tar = GzDecoder::new(&tarball[..]);
    let mut archive = Archive::new(tar);
    archive.unpack(output_path).map_err(|_| DownloadError::ExtractionFailed)?;

    Ok(())
}

pub fn download_database(database_path: Option<&Path>) -> Result<(), DownloadError> {
    // Download the tarball
    let url = "";
    let tarball_path = "database.tar.gz";
    download_and_extract_tarball(url, tarball_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_download_and_extract_tarball() {
        // Create a temporary directory to store the extracted files
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        // Download and extract a sample tarball
        let url = "https://example.com/sample.tar.gz";
        let result = download_and_extract_tarball(url, output_path.to_str().unwrap());

        // Assert that the function executed successfully
        assert!(result.is_ok());

        // Assert that the extracted files exist
        assert!(output_path.exists());
        assert!(output_path.join("file1.txt").exists());
        assert!(output_path.join("file2.txt").exists());

        // Clean up the temporary directory
        temp_dir.close().unwrap();
    }

    
    #[test]
    fn test_download_failure() {
        // Create a temporary directory to store the downloaded files
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        // Download and extract a non-existent tarball
        let url = "https://example.com/nonexistent.tar.gz";
        let result = download_and_extract_tarball(url, output_path.to_str().unwrap());

        // Assert that the function returns a DownloadFailed error
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            DownloadError::DownloadFailed
        );

        // Clean up the temporary directory
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_extraction_failure() {
        // Create a temporary directory to store the downloaded files
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        // Download and extract a tarball with invalid format
        let url = "https://example.com/invalid.tar.gz";
        let result = download_and_extract_tarball(url, output_path.to_str().unwrap());

        // Assert that the function returns an ExtractionFailed error
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            DownloadError::ExtractionFailed
        );

        // Clean up the temporary directory
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_download_database() {
        // Create a temporary directory to store the downloaded files
        let temp_dir = TempDir::new().unwrap();
        let database_path = temp_dir.path().join("database");

        // Download the database tarball
        let result = download_database(Some(database_path.as_path()));

        // Assert that the function executed successfully
        assert!(result.is_ok());

        // Assert that the downloaded files exist
        assert!(database_path.exists());
        assert!(database_path.join("data1.txt").exists());
        assert!(database_path.join("data2.txt").exists());

        // Clean up the temporary directory
        temp_dir.close().unwrap();
    }
}
