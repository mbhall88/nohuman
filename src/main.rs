use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::LazyLock;

use anyhow::{bail, Context, Result};
use clap::Parser;
use env_logger::Builder;
use log::{debug, error, info, warn, LevelFilter};
use nohuman::compression::CompressionFormat;
use nohuman::{
    check_path_exists, download::download_database, validate_db_directory, CommandRunner,
};

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
    #[arg(name = "INPUT", required_unless_present_any = &["check", "download"], value_parser = check_path_exists, verbatim_doc_comment)]
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
    #[arg(short = 'D', long = "db", value_name = "PATH", default_value = &**DEFAULT_DB_LOCATION)]
    database: PathBuf,

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
    if !args.database.exists() && !args.download && !args.check {
        bail!("Database does not exist. Use --download to download the database");
    }

    if args.download {
        info!("Downloading database...");
        download_database(&args.database).context("Failed to download database")?;
        info!("Database downloaded");
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
    let input = args.input.context("No input files provided")?;

    let temp_kraken_output =
        tempfile::NamedTempFile::new().context("Failed to create temporary kraken output file")?;
    let threads = args.threads.to_string();
    let db = validate_db_directory(&args.database)
        .map_err(|e| anyhow::anyhow!(e))?
        .to_string_lossy()
        .to_string();
    let mut kraken_cmd = vec![
        "--threads",
        &threads,
        "--db",
        &db,
        "--output",
        temp_kraken_output.path().to_str().unwrap(),
    ];
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

    // cleanup the temporary directory, but only issue a warning if it fails
    if let Err(e) = tmpdir.close() {
        warn!("Failed to remove temporary output directory: {}", e);
    }

    info!("Done.");

    Ok(())
}
