pub mod download;

use serde::Deserialize;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::PathBuf;
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
