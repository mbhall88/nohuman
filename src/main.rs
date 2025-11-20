use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use env_logger::Builder;
use log::{debug, error, info, warn, LevelFilter};
use nohuman::compression::CompressionFormat;
use nohuman::download::{self, download_database, DbSelection};
use nohuman::{check_path_exists, parse_confidence_score, validate_db_directory, CommandRunner};

static DEFAULT_DB_LOCATION: LazyLock<String> = LazyLock::new(|| {
    let home = dirs::home_dir().unwrap_or_default();
    home.join(".nohuman")
        .join("db")
        .to_string_lossy()
        .to_string()
});

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input file(s) to remove human reads from
    #[arg(name = "INPUT", required_unless_present_any = &["check", "download", "list_db_versions"], value_parser = check_path_exists, verbatim_doc_comment)]
    input: Option<Vec<PathBuf>>,

    /// First output file.
    ///
    /// Defaults to the name of the first input file with the suffix "nohuman" appended.
    /// e.g. "input_1.fastq" -> "input_1.nohuman.fq".
    /// Compression of the output file is determined by the file extension of the output file name.
    /// Or by using the `--output-type` option. If no output path is given, the same compression
    /// as the input file will be used.
    #[arg(short, long, name = "OUTPUT_1", verbatim_doc_comment)]
    pub out1: Option<PathBuf>,
    /// Second output file.
    ///
    /// Defaults to the name of the first input file with the suffix "nohuman" appended.
    /// e.g. "input_2.fastq" -> "input_2.nohuman.fq".
    /// Compression of the output file is determined by the file extension of the output file name.
    /// Or by using the `--output-type` option. If no output path is given, the same compression
    /// as the input file will be used.
    #[arg(short = 'O', long, name = "OUTPUT_2", verbatim_doc_comment)]
    pub out2: Option<PathBuf>,

    /// Check that all required dependencies are available and exit.
    #[arg(short, long)]
    check: bool,

    /// Download the database
    #[arg(short, long)]
    download: bool,

    /// Path to the database
    #[arg(
        short = 'D',
        long = "db",
        value_name = "PATH",
        default_value = &**DEFAULT_DB_LOCATION,
        env = "NOHUMAN_DB"
    )]
    database: PathBuf,

    /// Name of the database version to use (defaults to the newest installed). When used with
    /// `--download`, passing `all` downloads every available version.
    #[arg(long, value_name = "VERSION")]
    db_version: Option<String>,

    /// List available database versions and exit
    #[arg(long)]
    list_db_versions: bool,

    /// Output compression format. u: uncompressed; b: Bzip2; g: Gzip; x: Xz (Lzma); z: Zstd
    ///
    /// If not provided, the format will be inferred from the given output file name(s), or the
    /// format of the input file(s) if no output file name(s) are given.
    #[clap(short = 'F', long, value_name = "FORMAT", verbatim_doc_comment)]
    pub output_type: Option<CompressionFormat>,

    /// Number of threads to use in kraken2 and optional output compression. Cannot be 0.
    #[arg(short, long, value_name = "INT", default_value = "1")]
    threads: NonZeroU32,

    /// Output human reads instead of removing them
    #[arg(short = 'H', long = "human")]
    keep_human_reads: bool,

    /// Kraken2 minimum confidence score
    #[arg(short = 'C', long = "conf", value_name = "[0, 1]", default_value = "0.0", value_parser = parse_confidence_score)]
    confidence: f32,

    /// Write the Kraken2 read classification output to a file.
    #[arg(short, long, value_name = "FILE")]
    kraken_output: Option<PathBuf>,

    /// Write the Kraken2 report with aggregate counts/clade to file
    #[arg(short = 'r', long, value_name = "FILE")]
    kraken_report: Option<PathBuf>,

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

    if args.list_db_versions {
        let config = download::download_config().context("Failed to download database manifest")?;
        println!("Available databases:");
        for release in &config.databases {
            let mut labels = Vec::new();
            if config
                .default_version
                .as_ref()
                .is_some_and(|default| default == &release.version)
            {
                labels.push("default");
            }
            let label = if labels.is_empty() {
                String::new()
            } else {
                format!(" ({})", labels.join(", "))
            };
            println!(
                "- {}{} (added {}) -> {}",
                release.version, label, release.added, release.url
            );
        }
        return Ok(());
    }

    if args.download {
        let selection = match args.db_version.as_deref() {
            Some("all") => DbSelection::All,
            Some(version) => DbSelection::Version(version.to_string()),
            None => DbSelection::Latest,
        };
        info!("Downloading database...");
        let installs =
            download_database(&args.database, selection).context("Failed to download database")?;
        for install in installs {
            match validate_db_directory(&install.path) {
                Ok(actual) => info!("Database {} ready at {:?}", install.version, actual),
                Err(_) => info!("Database {} ready at {:?}", install.version, install.path),
            }
        }
        info!("Database download complete");
        if args.input.is_none() {
            info!("No input files provided. Exiting.");
            return Ok(());
        }
    }

    let kraken = CommandRunner::new("kraken2");

    let external_commands = vec![&kraken];

    let mut missing_commands = Vec::new();
    for cmd in external_commands {
        if !cmd.is_executable() {
            debug!("{} is not executable", cmd.command);
            missing_commands.push(cmd.command.to_owned());
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

    // error out if input files are not provided, otherwise unwrap to a variable
    let input = args.input.clone().context("No input files provided")?;

    let resolved_db = resolve_database(&args)?;
    if let Some(version) = &resolved_db.version {
        info!(
            "Using database version {} at {:?}",
            version, resolved_db.path
        );
    } else {
        info!("Using database at {:?}", resolved_db.path);
    }

    let kraken_output = args.kraken_output.unwrap_or(PathBuf::from("/dev/null"));
    let kraken_output = kraken_output.to_string_lossy();
    let threads = args.threads.to_string();
    let confidence = args.confidence.to_string();
    let db = resolved_db.path.to_string_lossy().to_string();
    let mut kraken_cmd = vec![
        "--threads",
        &threads,
        "--db",
        &db,
        "--output",
        &kraken_output,
        "--confidence",
        &confidence,
    ];

    if let Some(report_path) = args.kraken_report.as_ref().and_then(|p| p.to_str()) {
        kraken_cmd.extend(&["--report", report_path]);
    }

    match input.len() {
        0 => bail!("No input files provided"),
        2 => kraken_cmd.push("--paired"),
        i if i > 2 => bail!("Only one or two input files are allowed"),
        _ => {}
    }

    // safe to do this as we know the input vector is not empty
    let output_compression = if let Some(format) = args.output_type {
        Ok(format)
    } else if let Some(out1) = &args.out1 {
        CompressionFormat::from_path(out1)
    } else {
        let mut reader = std::io::BufReader::new(std::fs::File::open(&input[0])?);
        CompressionFormat::from_reader(&mut reader)
    }?;

    // create a temporary output directory in the current directory and don't delete it
    let tmpdir = tempfile::Builder::new()
        .prefix("nohuman")
        .tempdir_in(std::env::current_dir().unwrap())
        .context("Failed to create temporary directory")?;
    let outfile = if input.len() == 2 {
        tmpdir.path().join("kraken_out#.fq")
    } else {
        tmpdir.path().join("kraken_out.fq")
    };
    let outfile = outfile.to_string_lossy().to_string();

    if args.keep_human_reads {
        kraken_cmd.extend(&["--classified-out", &outfile]);
        info!("Keeping human reads...");
    } else {
        kraken_cmd.extend(&["--unclassified-out", &outfile]);
        info!("Removing human reads...");
    }

    kraken_cmd.extend(input.iter().map(|p| p.to_str().unwrap()));
    debug!("Running kraken2...");
    debug!("With arguments: {:?}", &kraken_cmd);
    kraken.run(&kraken_cmd).context("Failed to run kraken2")?;
    info!("Kraken2 finished. Organising output...");

    let outputs = if input.len() == 2 {
        let out1 = args.out1.unwrap_or_else(|| {
            let parent = input[0].parent().unwrap();
            // get the part of the file name before the extension.
            // if the file is compressed, the extension will be .gz, we want to remove this first before getting the file stem
            let ext = CompressionFormat::from_path(&input[0])
                .unwrap_or_default()
                .to_string();
            let fname = if input[0].extension().unwrap_or_default() == ext.as_str() {
                let no_ext = input[0].with_extension("");
                no_ext.file_stem().unwrap().to_owned()
            } else {
                input[0].file_stem().unwrap().to_owned()
            };
            let fname = format!("{}.nohuman.fq", fname.to_string_lossy());
            let fname = parent.join(fname);
            output_compression.add_extension(&fname)
        });
        let out2 = args.out2.unwrap_or_else(|| {
            let parent = input[1].parent().unwrap();
            // get the part of the file name before the extension.
            // if the file is compressed, the extension will be .gz, we want to remove this first before getting the file stem
            let ext = CompressionFormat::from_path(&input[1])
                .unwrap_or_default()
                .to_string();
            let fname = if input[1].extension().unwrap_or_default() == ext.as_str() {
                let no_ext = input[1].with_extension("");
                no_ext.file_stem().unwrap().to_owned()
            } else {
                input[1].file_stem().unwrap().to_owned()
            };
            let fname = format!("{}.nohuman.fq", fname.to_string_lossy());
            let fname = parent.join(fname);
            output_compression.add_extension(&fname)
        });
        let tmpout1 = tmpdir.path().join("kraken_out_1.fq");
        let tmpout2 = tmpdir.path().join("kraken_out_2.fq");
        vec![(tmpout1, out1), (tmpout2, out2)]
        // move the output files to the correct location
        // std::fs::rename(tmpout1, &out1).unwrap();
        // std::fs::rename(tmpout2, &out2).unwrap();
        // info!("Output files written to: {:?} and {:?}", &out1, &out2);
    } else {
        let out1 = args.out1.unwrap_or_else(|| {
            let parent = input[0].parent().unwrap();
            // get the part of the file name before the extension.
            // if the file is compressed, the extension will be .gz, we want to remove this first before getting the file stem
            let ext = CompressionFormat::from_path(&input[0])
                .unwrap_or_default()
                .to_string();
            let fname = if input[0].extension().unwrap_or_default() == ext.as_str() {
                let no_ext = input[0].with_extension("");
                no_ext.file_stem().unwrap().to_owned()
            } else {
                input[0].file_stem().unwrap().to_owned()
            };
            let fname = format!("{}.nohuman.fq", fname.to_string_lossy());
            let fname = parent.join(fname);
            output_compression.add_extension(&fname)
        });
        let tmpout1 = tmpdir.path().join("kraken_out.fq");
        vec![(tmpout1, out1)]
        // move the output files to the correct location
        // std::fs::rename(tmpout1, &out1).unwrap();
        // info!("Output file written to: {:?}", &out1);
    };

    // if we have one output file and multiple threads, we pass all threads to the compression command
    // if we have two output files, we pass half the threads to each compression command
    let threads = if outputs.len() == 1 {
        args.threads.get()
    } else {
        args.threads.get() / 2
    };

    // if we have two output files and two or more threads, compress them in parallel
    if outputs.len() == 2 && threads > 1 {
        let mut handles = Vec::new();
        for (input, output) in outputs {
            let handle = std::thread::spawn(move || {
                info!("Writing output file to: {:?}", &output);
                output_compression.compress(&input, &output, threads)
            });
            handles.push(handle);
        }
        for handle in handles {
            handle
                .join()
                .map_err(|e| anyhow::anyhow!("Thread panicked when writing output: {:?}", e))??;
        }
    } else {
        for (input, output) in outputs {
            output_compression.compress(&input, &output, threads)?;
            info!("Output file written to: {:?}", &output);
        }
    }

    if kraken_output != "/dev/null" {
        info!("Kraken output file written to: {:?}", &kraken_output);
    }

    if let Some(report_path) = &args.kraken_report {
        info!("Kraken report file written to: {:?}", &report_path);
    }

    // cleanup the temporary directory, but only issue a warning if it fails
    if let Err(e) = tmpdir.close() {
        warn!("Failed to remove temporary output directory: {}", e);
    }

    info!("Done.");

    Ok(())
}

struct ResolvedDatabase {
    path: PathBuf,
    version: Option<String>,
}

fn resolve_database(args: &Args) -> Result<ResolvedDatabase> {
    if let Some(version) = &args.db_version {
        if version == "all" {
            bail!("Cannot run with `--db-version all`. Use `--download --db-version all` to download every database.");
        }
        let installed = download::find_installed_database(&args.database, version).ok_or_else(
            || {
                anyhow!(
                    "Database version '{}' is not installed under {:?}. Run `nohuman --download --db-version {}` to download it.",
                    version,
                    args.database,
                    version
                )
            },
        )?;
        let path = validate_db_directory(&installed.path).map_err(|e| anyhow!(e))?;
        return Ok(ResolvedDatabase {
            path,
            version: Some(installed.version),
        });
    }

    if let Ok(path) = validate_db_directory(&args.database) {
        return Ok(ResolvedDatabase {
            path,
            version: None,
        });
    }

    if let Some(installed) = download::latest_installed_database(&args.database) {
        let path = validate_db_directory(&installed.path).map_err(|e| anyhow!(e))?;
        return Ok(ResolvedDatabase {
            path,
            version: Some(installed.version),
        });
    }

    Err(anyhow!(
        "Database does not exist at {:?}. Run `nohuman --download` to fetch one.",
        args.database
    ))
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::Parser;
    use std::env;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn database_path_can_be_set_via_env_var() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp = tempdir().unwrap();
        env::set_var("NOHUMAN_DB", temp.path());
        let args = Args::try_parse_from(["nohuman", "--check"]).unwrap();
        assert_eq!(args.database, temp.path());
        env::remove_var("NOHUMAN_DB");
    }
}
