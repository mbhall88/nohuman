use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use env_logger::Builder;
use lazy_static::lazy_static;
use log::{debug, error, info, LevelFilter};
use nohuman::{download::download_database, CommandRunner};

lazy_static! {
    static ref DEFAULT_DB_LOCATION: String = {
        let home = dirs::home_dir().expect("Could not find home directory");
        home.join(".nohuman")
            .join("db")
            .to_str()
            .unwrap()
            .to_string()
    };
}

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
    #[arg(short = 'D', long = "db", value_name = "PATH", default_value = &**DEFAULT_DB_LOCATION)]
    database: PathBuf,

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
    if !args.database.exists() {
        bail!("Database does not exist. Use --download to download the database");
    }

    if args.download {
        info!("Downloading database...");
        download_database(&args.database).context("Failed to download database")?;
        info!("Database downloaded");
    }

    let kraken = CommandRunner::new("kraken2");

    let external_commands = vec![kraken];

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
