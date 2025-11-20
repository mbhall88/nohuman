use crate::validate_db_directory;
use async_std::task;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use jiff::civil::Date;
use log::{debug, info};
use reqwest::blocking::get;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tar::Archive;
use thiserror::Error;

const CONFIG_URL: &str = "https://raw.githubusercontent.com/mbhall88/nohuman/main/config.toml";
const METADATA_FILE: &str = "nohuman-db.toml";
const LEGACY_ADDED_DATE: &str = "1970-01-01";

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

    #[error("Failed to parse metadata for database {0}")]
    MetadataParseFailed(String),

    #[error("Database version '{0}' not found in manifest")]
    UnknownDatabaseVersion(String),

    #[error("No databases are defined in the manifest")]
    NoDatabasesAvailable,

    #[error("Failed to parse database release date '{0}'")]
    InvalidDate(String),

    #[error("Failed to compute MD5 hash")]
    Md5Error,

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub default_version: Option<String>,
    pub databases: Vec<DatabaseRelease>,
}

impl DatabaseConfig {
    pub fn find_release(&self, version: &str) -> Option<&DatabaseRelease> {
        self.databases.iter().find(|db| db.version == version)
    }

    pub fn latest_release(&self) -> Option<&DatabaseRelease> {
        if let Some(default_version) = &self.default_version {
            return self.find_release(default_version);
        }
        self.databases
            .iter()
            .max_by(|a, b| date_sort_key(&a.added).cmp(&date_sort_key(&b.added)))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseRelease {
    pub version: String,
    pub url: String,
    pub md5: String,
    pub added: String,
}

#[derive(Debug, Clone)]
pub enum DbSelection {
    Latest,
    Version(String),
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledMetadata {
    version: String,
    added: String,
}

#[derive(Debug, Clone)]
pub struct InstalledDatabase {
    pub version: String,
    pub path: PathBuf,
    pub added: String,
}

/// Downloads databases according to the provided selection and returns the installed entries.
pub fn download_database(
    database_root: &Path,
    selection: DbSelection,
) -> Result<Vec<InstalledDatabase>, DownloadError> {
    let config = download_config()?;
    let releases: Vec<&DatabaseRelease> = match selection {
        DbSelection::Latest => {
            let release = config
                .latest_release()
                .ok_or(DownloadError::NoDatabasesAvailable)?;
            vec![release]
        }
        DbSelection::Version(version) => {
            let release = config
                .find_release(&version)
                .ok_or_else(|| DownloadError::UnknownDatabaseVersion(version.clone()))?;
            vec![release]
        }
        DbSelection::All => config.databases.iter().collect(),
    };

    if releases.is_empty() {
        return Err(DownloadError::NoDatabasesAvailable);
    }

    let mut installed = Vec::new();
    for release in releases {
        let db = download_release(database_root, release)?;
        installed.push(db);
    }

    Ok(installed)
}

pub fn download_config() -> Result<DatabaseConfig, DownloadError> {
    let mut response = get(CONFIG_URL).map_err(|_| DownloadError::ConfigDownloadFailed)?;
    let mut config_content = String::new();
    response
        .read_to_string(&mut config_content)
        .map_err(|_| DownloadError::ConfigDownloadFailed)?;

    let config: DatabaseConfig =
        toml::from_str(&config_content).map_err(|_| DownloadError::ConfigParseFailed)?;
    for release in &config.databases {
        parse_added_date(&release.added)?;
    }
    Ok(config)
}

pub fn installed_databases(database_root: &Path) -> Vec<InstalledDatabase> {
    let mut installed = Vec::new();

    if let Ok(entries) = fs::read_dir(database_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            match read_metadata(&path) {
                Ok(meta) => {
                    if validate_db_directory(&path).is_ok() {
                        installed.push(InstalledDatabase {
                            version: meta.version,
                            path,
                            added: meta.added,
                        });
                    } else {
                        debug!(
                            "Skipping {:?} because the required Kraken files were not found",
                            path
                        );
                    }
                }
                Err(err) => {
                    debug!(
                        "Skipping {:?} because metadata could not be read: {:?}",
                        path, err
                    );
                }
            }
        }
    }

    // Legacy installs have the database directly inside `database_root`.
    if validate_db_directory(database_root).is_ok() && read_metadata(database_root).is_err() {
        installed.push(InstalledDatabase {
            version: "legacy".to_string(),
            path: database_root.to_path_buf(),
            added: LEGACY_ADDED_DATE.to_string(),
        });
    }

    installed
}

pub fn latest_installed_database(database_root: &Path) -> Option<InstalledDatabase> {
    installed_databases(database_root)
        .into_iter()
        .max_by(|a, b| date_sort_key(&a.added).cmp(&date_sort_key(&b.added)))
}

pub fn find_installed_database(database_root: &Path, version: &str) -> Option<InstalledDatabase> {
    installed_databases(database_root)
        .into_iter()
        .find(|db| db.version == version)
}

fn download_release(
    database_root: &Path,
    release: &DatabaseRelease,
) -> Result<InstalledDatabase, DownloadError> {
    fs::create_dir_all(database_root)?;
    let target_dir = database_root.join(&release.version);

    if let Some(existing) = find_installed_database(database_root, &release.version) {
        info!(
            "Database version {} already present at {:?}, skipping download",
            release.version, existing.path
        );
        return Ok(existing);
    }

    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)?;
    }
    fs::create_dir_all(&target_dir)?;

    download_and_extract_tarball(&release.url, &target_dir, &release.md5)?;

    write_metadata(
        &target_dir,
        &InstalledMetadata {
            version: release.version.clone(),
            added: release.added.clone(),
        },
    )?;

    validate_db_directory(&target_dir).map_err(|_| DownloadError::ExtractionFailed)?;

    info!("Installed database {} at {:?}", release.version, target_dir);

    Ok(InstalledDatabase {
        version: release.version.clone(),
        path: target_dir,
        added: release.added.clone(),
    })
}

fn write_metadata(dir: &Path, metadata: &InstalledMetadata) -> Result<(), DownloadError> {
    let content = toml::to_string(metadata).map_err(|_| DownloadError::ConfigParseFailed)?;
    fs::write(dir.join(METADATA_FILE), content)?;
    Ok(())
}

fn read_metadata(dir: &Path) -> Result<InstalledMetadata, DownloadError> {
    let path = dir.join(METADATA_FILE);
    let content = fs::read_to_string(&path)?;
    toml::from_str(&content)
        .map_err(|_| DownloadError::MetadataParseFailed(path.to_string_lossy().to_string()))
}

fn parse_added_date(raw: &str) -> Result<Date, DownloadError> {
    Date::from_str(raw).map_err(|_| DownloadError::InvalidDate(raw.to_string()))
}

fn date_sort_key(raw: &str) -> Date {
    parse_added_date(raw).unwrap_or_else(|_| {
        parse_added_date(LEGACY_ADDED_DATE).expect("legacy date string is valid")
    })
}

/// Compute md5 without buffering the whole file.
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
    Ok(format!("{result:x}",))
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
    let tarball_path = tempfile::NamedTempFile::new().map_err(DownloadError::IoError)?;
    task::block_on(download_from_url(url, tarball_path.path()))?;

    let md5_hash = compute_md5(tarball_path.path())?;
    if md5_hash != md5 {
        return Err(DownloadError::Md5Mismatch);
    }

    let tarball = File::open(tarball_path.path()).map_err(DownloadError::IoError)?;
    let tar = GzDecoder::new(&tarball);
    let mut archive = Archive::new(tar);
    archive
        .unpack(output_path)
        .map_err(|_| DownloadError::ExtractionFailed)?;

    fs::remove_file(tarball_path.path()).map_err(DownloadError::IoError)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    pub fn check_internet_connection(timeout: std::time::Duration) -> bool {
        use std::net::{SocketAddr, TcpStream};

        let addr = "8.8.8.8:53".parse::<SocketAddr>().unwrap();
        TcpStream::connect_timeout(&addr, timeout).is_ok()
    }

    #[test]
    fn test_download_and_extract_tarball() {
        if !check_internet_connection(std::time::Duration::from_secs(2)) {
            return;
        }
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        let url = "https://github.com/mbhall88/rasusa/releases/download/0.7.1/rasusa-0.7.1-x86_64-unknown-linux-gnu.tar.gz";
        let md5 = "6c60c417646084eac81fc23a85e9fbc2";
        let result = download_and_extract_tarball(url, &output_path, md5);

        assert!(result.is_ok());

        let output_path = output_path.join("rasusa-0.7.1-x86_64-unknown-linux-gnu");
        assert!(output_path.exists());
        assert!(output_path.join("LICENSE").exists());
        assert!(output_path.join("rasusa").exists());
        assert!(output_path.join("README.md").exists());
        assert!(output_path.join("CHANGELOG.md").exists());

        temp_dir.close().unwrap();
    }

    #[test]
    fn test_download_and_extract_tarball_md5_mismatch() {
        if !check_internet_connection(std::time::Duration::from_secs(2)) {
            return;
        }
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        let url = "https://github.com/mbhall88/rasusa/releases/download/0.7.1/rasusa-0.7.1-x86_64-unknown-linux-gnu.tar.gz";
        let md5 = "foo";
        let result = download_and_extract_tarball(url, &output_path, md5);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            DownloadError::Md5Mismatch.to_string()
        );

        temp_dir.close().unwrap();
    }

    #[test]
    fn test_download_failure() {
        if !check_internet_connection(std::time::Duration::from_secs(2)) {
            return;
        }
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        let url = "https://example.com/nonexistent.tar.gz";
        let md5 = "foo";
        let result = download_and_extract_tarball(url, &output_path, md5);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            DownloadError::DownloadFailed.to_string()
        );

        temp_dir.close().unwrap();
    }

    #[test]
    fn test_extraction_failure() {
        if !check_internet_connection(std::time::Duration::from_secs(2)) {
            return;
        }
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("output");

        let url = "https://raw.githubusercontent.com/mbhall88/rasusa/fa7e87b843419151cc4716c670adbb28544979b1/Cargo.toml";
        let md5 = "95143b02c21cc9ce1980645d2db69937";
        let result = download_and_extract_tarball(url, &output_path, md5);

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            DownloadError::ExtractionFailed.to_string()
        );

        temp_dir.close().unwrap();
    }

    #[test]
    fn test_compute_md5() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("LICENSE")
            .canonicalize()
            .unwrap();

        let actual = compute_md5(&path).unwrap();
        let expected = "31cf5fcf677d471a05001d8891332ae1".to_string();
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_metadata_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let metadata = InstalledMetadata {
            version: "HPRC.r1".to_string(),
            added: "2024-01-01".to_string(),
        };
        write_metadata(temp_dir.path(), &metadata).unwrap();
        let parsed = read_metadata(temp_dir.path()).unwrap();
        assert_eq!(parsed.version, metadata.version);
        assert_eq!(parsed.added, metadata.added);
    }

    #[test]
    fn test_installed_databases_legacy() {
        let temp_dir = TempDir::new().unwrap();
        for file in ["hash.k2d", "opts.k2d", "taxo.k2d"] {
            fs::write(temp_dir.path().join(file), b"").unwrap();
        }
        let installed = installed_databases(temp_dir.path());
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].version, "legacy");
    }

    #[test]
    fn test_latest_installed_database() {
        let temp_dir = TempDir::new().unwrap();
        let v1 = temp_dir.path().join("HPRC.r1");
        let v2 = temp_dir.path().join("HPRC.r2");
        fs::create_dir_all(v1.join("db")).unwrap();
        fs::create_dir_all(v2.join("db")).unwrap();
        for dir in [&v1, &v2] {
            for file in ["hash.k2d", "opts.k2d", "taxo.k2d"] {
                fs::write(dir.join("db").join(file), b"").unwrap();
            }
        }
        write_metadata(
            &v1,
            &InstalledMetadata {
                version: "HPRC.r1".to_string(),
                added: "2023-01-01".to_string(),
            },
        )
        .unwrap();
        write_metadata(
            &v2,
            &InstalledMetadata {
                version: "HPRC.r2".to_string(),
                added: "2024-01-01".to_string(),
            },
        )
        .unwrap();

        let latest = latest_installed_database(temp_dir.path()).unwrap();
        assert_eq!(latest.version, "HPRC.r2");
    }
}
