use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use env_logger::Builder;
use log::{debug, error, info, warn, LevelFilter};
use nohuman::{download::download_database, CommandRunner};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file(s) to remove human reads from
    #[arg(name = "INPUT", required_unless_present_any = &["check", "download"])]
    input: Option<Vec<PathBuf>>,

    /// Check that all required dependencies are available
    #[arg(short, long)]
    check: bool,

    /// Download the database
    #[arg(short, long)]
    download: bool,

    /// Path to the database
    #[arg(short = 'D', long = "db", value_name = "PATH")]
    database: Option<PathBuf>,

    /// Set the logging level to verbose
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logger
    let log_lvl = if args.verbose {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let mut log_builder = Builder::new();
    log_builder
        .filter(None, log_lvl)
        .filter_module("reqwest", LevelFilter::Off)
        .format_module_path(false)
        .format_target(false)
        .init();

    // Check if the database exists
    if let Some(database_path) = &args.database {
        if !Path::new(database_path).exists() && !args.download {
            bail!("Database does not exist at the provided path");
        }
    } else {
        // Check if the default location exists
        todo!("make this a global option");
        let default_database_path = Path::new("default_database.db");
        if !default_database_path.exists() && !args.download {
            bail!("Default database does not exist");
        }
    }

    if args.download {
        info!("Downloading database...");
        download_database(args.database.as_deref())?;
        info!("Database downloaded");
    }

    let conda = CommandRunner::new("conda");

    let external_commands = vec![conda];

    let mut missing_commands = Vec::new();
    for cmd in external_commands {
        if !cmd.is_executable() {
            debug!("{} is not executable", cmd.command);
            missing_commands.push(cmd.command);
        } else {
            debug!("{} is executable", cmd.command);
        }
    }

    if !missing_commands.is_empty() {
        error!("The following dependencies are missing:");
        for cmd in missing_commands {
            error!("{}", cmd);
        }
        bail!("Missing dependencies");
    }

    if args.check {
        info!("All dependencies are available");
        return Ok(());
    }

    Ok(())
}
