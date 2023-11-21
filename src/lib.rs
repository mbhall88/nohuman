pub mod download;

use std::io::{self, Write};
use std::process::Command;

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
        Command::new("command")
            .args(["-v", &self.command])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

// add tests for each of the CommandRunner methods
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
}
