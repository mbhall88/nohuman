use crate::Config;
use async_std::task;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::blocking::get;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use tar::Archive;
use thiserror::Error;

// create a variable to store the url for the config file
const CONFIG_URL: &str = "https://raw.githubusercontent.com/mbhall88/nohuman/main/config.toml";

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("Failed to download the tarball")]
    DownloadFailed,

    #[error("Tarball MD5 hash does not match the expected value")]
    Md5Mismatch,

    #[error("Failed to extract the tarball")]
    ExtractionFailed,

    #[error("Failed to download the config file")]
    ConfigDownloadFailed,

    #[error("Failed to parse the config file")]
    ConfigParseFailed,

    #[error("Failed to compute MD5 hash")]
    Md5Error,

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
}

/// function to compute md5 without reading whole file into memory
fn compute_md5(path: &Path) -> Result<String, DownloadError> {
    let mut file = fs::File::open(path).map_err(DownloadError::IoError)?;
    let mut hasher = md5::Context::new();
    let mut buffer = [0; 1024];
    loop {
        let n = file.read(&mut buffer).map_err(DownloadError::IoError)?;
        if n == 0 {
            break;
        }
        hasher.consume(&buffer[..n]);
    }
    let result = hasher.compute();
    Ok(format!("{:x}", result))
}

async fn download_from_url(url: &str, dest: &Path) -> Result<(), DownloadError> {
    let response = reqwest::get(url)
        .await
        .map_err(DownloadError::ReqwestError)?;

    if response.status() != reqwest::StatusCode::OK {
        return Err(DownloadError::DownloadFailed);
    }

    let content_length = response.content_length().unwrap_or(0);
    let progress_bar = ProgressBar::new(content_length);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut file = File::create(dest).map_err(DownloadError::IoError)?;

    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item?;
        file.write_all(&chunk).map_err(DownloadError::IoError)?;
        progress_bar.inc(chunk.len() as u64);
    }

    progress_bar.finish();
    Ok(())
}

fn download_and_extract_tarball(
    url: &str,
    output_path: &Path,
    md5: &str,
) -> Result<(), DownloadError> {
    // Create a temporary file to store the downloaded tarball
    let tarball_path = tempfile::NamedTempFile::new().map_err(DownloadError::IoError)?;
    task::block_on(download_from_url(url, tarball_path.path()))?;

    // Check the MD5 hash of the tarball
    let md5_hash = compute_md5(tarball_path.path())?;
    if md5_hash != md5 {
        return Err(DownloadError::Md5Mismatch);
    }

    // Extract the tarball to the output path
    let tarball = File::open(tarball_path.path()).map_err(DownloadError::IoError)?;
    let tar = GzDecoder::new(&tarball);
    let mut archive = Archive::new(tar);
    archive
        .unpack(output_path)
        .map_err(|_| DownloadError::ExtractionFailed)?;

    // remove the temporary tarball file
    fs::remove_file(tarball_path.path()).map_err(DownloadError::IoError)?;

    Ok(())
}

pub fn download_database(database_path: &Path) -> Result<(), DownloadError> {
    let config = download_config()?;
    download_and_extract_tarball(&config.database_url, database_path, &config.database_md5)?;
    Ok(())
}

fn download_config() -> Result<Config, DownloadError> {
    // Download the config file
    let mut response = get(CONFIG_URL).map_err(|_| DownloadError::ConfigDownloadFailed)?;
    let mut config_content = String::new();
    response
        .read_to_string(&mut config_content)
        .map_err(|_| DownloadError::ConfigDownloadFailed)?;

    // Parse the TOML content into a config struct
    let config: Config =
        toml::from_str(&config_content).map_err(|_| DownloadError::ConfigParseFailed)?;

    Ok(config)
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
        let url = "https://github.com/mbhall88/rasusa/releases/download/0.7.1/rasusa-0.7.1-x86_64-unknown-linux-gnu.tar.gz";
        let md5 = "6c60c417646084eac81fc23a85e9fbc2";
        let result = download_and_extract_tarball(url, &output_path, md5);

        // Assert that the function executed successfully
        assert!(result.is_ok());

        // Assert that the extracted files exist
        let output_path = output_path.join("rasusa-0.7.1-x86_64-unknown-linux-gnu");
        assert!(output_path.exists());
        assert!(output_path.join("LICENSE").exists());
        assert!(output_path.join("rasusa").exists());
        assert!(output_path.join("README.md").exists());
        assert!(output_path.join("CHANGELOG.md").exists());

        // Clean up the temporary directory
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_download_and_extract_tarball_md5_mismatch() {
        // Create a temporary directory to store the extracted files
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        // Download and extract a sample tarball
        let url = "https://github.com/mbhall88/rasusa/releases/download/0.7.1/rasusa-0.7.1-x86_64-unknown-linux-gnu.tar.gz";
        let md5 = "foo";
        let result = download_and_extract_tarball(url, &output_path, md5);

        // Assert that the function executed successfully
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            DownloadError::Md5Mismatch.to_string()
        );

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
        let md5 = "foo";
        let result = download_and_extract_tarball(url, &output_path, md5);

        // Assert that the function returns a DownloadFailed error
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            DownloadError::DownloadFailed.to_string()
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
        let url = "https://raw.githubusercontent.com/mbhall88/rasusa/main/Cargo.toml";
        let md5 = "77c811c1264306e607aff057420cf354";
        let result = download_and_extract_tarball(url, &output_path, md5);

        // Assert that the function returns an ExtractionFailed error
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            DownloadError::ExtractionFailed.to_string()
        );

        // Clean up the temporary directory
        temp_dir.close().unwrap();
    }

    #[test]
    fn test_compute_md5() {
        // path to the repository's LICENSE file
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("LICENSE")
            .canonicalize()
            .unwrap();

        let actual = compute_md5(&path).unwrap();
        let expected = "31cf5fcf677d471a05001d8891332ae1".to_string();
        assert_eq!(actual, expected);
    }
}
