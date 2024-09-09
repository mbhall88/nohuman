# NoHuman

[![Rust CI](https://github.com/mbhall88/nohuman/actions/workflows/ci.yaml/badge.svg)](https://github.com/mbhall88/nohuman/actions/workflows/ci.yaml)
[![Crates.io](https://img.shields.io/crates/v/nohuman.svg)](https://crates.io/crates/nohuman)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![github release version](https://img.shields.io/github/v/release/mbhall88/nohuman)](https://github.com/mbhall88/nohuman/releases)
[![DOI:10.1093/gigascience/giae010](https://img.shields.io/badge/citation-10.1093/gigascience/giae010-blue)][paper]


👤➡️🚫 **Remove human reads from a sequencing run** 👤➡️🚫

`nohuman` removes human reads from sequencing reads by classifying them with [kraken2][kraken] against a custom database built from all of the genomes in the Human Pangenome Reference Consortium's (HPRC) [first draft human pangenome reference](https://doi.org/10.1038/s41586-023-05896-x). It can take any type of sequencing technology. Read more about the development of this method [here][paper].

- [NoHuman](#nohuman)
  - [Install](#install)
    - [Conda (recommended)](#conda-recommended)
    - [Precompiled binary](#precompiled-binary)
    - [Cargo](#cargo)
    - [Container](#container)
      - [`singularity`](#singularity)
      - [`docker`](#docker)
    - [Build from source](#build-from-source)
  - [Usage](#usage)
    - [Download the database](#download-the-database)
    - [Check dependecies are available](#check-dependecies-are-available)
    - [Remove human reads](#remove-human-reads)
    - [Full usage](#full-usage)
  - [Alternates](#alternates)
  - [Cite](#cite)

## Install

### Conda (recommended)

[![Conda (channel only)](https://img.shields.io/conda/vn/bioconda/nohuman)](https://anaconda.org/bioconda/nohuman)
[![bioconda version](https://anaconda.org/bioconda/nohuman/badges/platforms.svg)](https://anaconda.org/bioconda/nohuman)
![Conda](https://img.shields.io/conda/dn/bioconda/nohuman)

```shell
$ conda install -c bioconda nohuman
```


### Precompiled binary

Note: you will need to [install kraken2][kraken] yourself using this install method.

```shell
curl -sSL nohuman.mbh.sh | sh
# or with wget
wget -nv -O - nohuman.mbh.sh | sh
```

You can also pass options to the script like so

```
$ curl -sSL nohuman.mbh.sh | sh -s -- --help
install.sh [option]

Fetch and install the latest version of nohuman, if nohuman is already
installed it will be updated to the latest version.

Options
        -V, --verbose
                Enable verbose output for the installer

        -f, -y, --force, --yes
                Skip the confirmation prompt during installation

        -p, --platform
                Override the platform identified by the installer [default: apple-darwin]

        -b, --bin-dir
                Override the bin installation directory [default: /usr/local/bin]

        -a, --arch
                Override the architecture identified by the installer [default: x86_64]

        -B, --base-url
                Override the base URL used for downloading releases [default: https://github.com/mbhall88/nohuman/releases]

        -h, --help
                Display this help message
```


### Cargo

![Crates.io](https://img.shields.io/crates/d/nohuman)

Note: you will need to [install kraken2][kraken] yourself using this install method.

```shell
$ cargo install nohuman
```

### Container

Docker images are hosted at [quay.io].

#### `singularity`

Prerequisite: [`singularity`][singularity]

```shell
$ URI="docker://quay.io/mbhall88/nohuman"
$ singularity exec "$URI" nohuman --help
```

The above will use the latest version. If you want to specify a version then use a
[tag][quay.io] (or commit) like so.

```shell
$ VERSION="0.1.1"
$ URI="docker://quay.io/mbhall88/nohuman:${VERSION}"
```

#### `docker`

[![Docker Repository on Quay](https://quay.io/repository/mbhall88/nohuman/status "Docker Repository on Quay")](https://quay.io/repository/mbhall88/nohuman)

Prerequisite: [`docker`][docker]

```shhell
$ docker pull quay.io/mbhall88/nohuman
$ docker run quay.io/mbhall88/nohuman nohuman --help
```

You can find all the available tags on the [quay.io repository][quay.io].

### Build from source

Note: you will need to [install kraken2][kraken] yourself using this install method.

```shell
$ git clone https://github.com/mbhall88/nohuman.git
$ cd nohuman
$ cargo build --release
$ target/release/nohuman -h
```


## Usage

### Download the database

```
$ nohuman -d
```

by default, this will place the database in `$HOME/.nohuman/db`. If you want to download it somewhere else, use the `--db` option.

### Check dependencies are available

```
$ nohuman -c
[2023-12-14T04:10:46Z INFO ] All dependencies are available
```

### Remove human reads

```
$ nohuman -t 4 in.fq
```

this will pass 4 threads to kraken2 and output the clean reads as `in.nohuman.fq`.

You can specify where to write the output file with `-o`

```
$ nohuman -t 4 -o clean.fq in.fq
```

If you have paired-end Illumina reads

```
$ nohuman -t 4 in_1.fq in_2.fq
```

or to specify a different path for the output


```
$ nohuman -t 4 --out1 clean_1.fq --out2 clean_2.fq in_1.fq in_2.fq
```

> [!TIP]
> If the input reads are compressed then the output will also be compressed by default. Likewise, if the input reads are not compressed, then the output reads will also be uncompressed by default. These behaviours can be overruled by explicitly specifying output filenames.

The detection of compression, and the desire to compress the outputs, is based on the filename extensions. Supported formats:
* gzip: '.gz'
* Block GZip Format (BGZF): '.bgz' or '.gz'
* bzip2: '.bz2'
* Zstandard: '.zst' or '.zstd'
* xz / LZMA: '.xz' or '.lzma'

Compressed in, compressed out:

```
$ nohuman -t 4 in_1.fq.gz in_2.fq.gz
```

```
$ nohuman -t 4 -o out_1.fq.gz -O out_2.fq.gz in_1.fq.gz in_2.fq.gz
```

Compressed in, uncompressed out:

```
$ nohuman -t 4 -o out_1.fq -O out_2.fq in_1.fq.gz in_2.fq.gz
```

Uncompressed in, uncompressed out:

```
$ nohuman -t 4 in_1.fq in_2.fq
```

```
$ nohuman -t 4 -o out_1.fq -O out_2.fq in_1.fq in_2.fq
```

Uncompressed in, compressed out:

```
$ nohuman -t 4 -o out_1.fq.gz -O out_2.fq.gz in_1.fq in_2.fq
```

Different compression format:

```
$ nohuman -t 4 -o out_1.fq.zst -O out_2.fq.zst in_1.fq in_2.fq
```

### Logging
The direct stderr logging of `kraken2` can be saved to file (plain text) using the `-l` / `--kraken2-log` option of `nohuman`. Overall statistics for the run can be saved to file (JSON format) using the `-s` / `--stats` option of `nohuman`.

### Short usage

```
$ nohuman -h
Remove human reads from a sequencing run

Usage: nohuman [OPTIONS] [INPUT]...

Arguments:
  [INPUT]...  Input file(s) to remove human reads from.

Options:
  -o, --out1 <OUTPUT_1>            First output file.
  -O, --out2 <OUTPUT_2>            Second output file.
  -c, --check                      Check that all required dependencies are available and exit.
  -d, --download                   Download the database required for the process.
  -D, --db <PATH>                  Path to the database. [default: /Users/charles/.nohuman/db]
  -l, --kraken2-log <PATH>         Write `kraken2` logging information to filename specified here.
  -t, --threads <INT>              Number of threads to use in kraken2  [default: 1]
      --compression-threads <INT>  Number of threads to use for compression.
      --overwrite                  Allow overwriting of existing output files.
  -v, --verbose                    Set the `nohuman` logging level to verbose
  -s, --stats <STATS_FILE>         Generate a stats file (JSON format) with run information
  -h, --help                       Print help (see more with '--help')
  -V, --version                    Print version```

### Full usage
```
$ nohuman --help
Remove human reads from a sequencing run

Usage: nohuman [OPTIONS] [INPUT]...

Arguments:
  [INPUT]...
          Input file(s) to remove human reads from.

          This is a required argument unless `--check` or `--download` is specified.

Options:
  -o, --out1 <OUTPUT_1>
          First output file.

          Defaults to the name of the first input file with the suffix "nohuman" appended.
          e.g., "input_1.fastq.gz" -> "input_1.nohuman.fq.gz".
          If the file stem is one of `.gz`, `.bgz`, `.xz`, `.zst`, the output will be
          compressed accordingly.

  -O, --out2 <OUTPUT_2>
          Second output file.

          Defaults to the name of the second input file with the suffix "nohuman" appended.
          e.g., "input_2.fastq.gz" -> "input_2.nohuman.fq.gz".
          If the file stem is one of `.gz`, `.bgz`, `.xz`, `.zst`, the output will be
          compressed accordingly.

  -c, --check
          Check that all required dependencies are available and exit.

  -d, --download
          Download the database required for the process.

  -D, --db <PATH>
          Path to the database.

          Defaults to the database location specified in the home directory.

          [default: /home/mihall/.nohuman/db]

  -l, --kraken2-log <PATH>
          Write `kraken2` logging information to filename specified here.

          If not specified, no `kraken2` log is saved.

  -t, --threads <INT>
          Number of threads to use in kraken2

          [default: 1]

      --compression-threads <INT>
          Number of threads to use for compression.

          Defaults to the same value as `--threads` if not specified by the user.

      --overwrite
          Allow overwriting of existing output files.

          If not provided, the process will error out if the output file(s) already exist.

  -v, --verbose
          Set the `nohuman` logging level to verbose

  -s, --stats <STATS_FILE>
          Generate a stats file (JSON format) with run information

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## Alternates

[Hostile](https://github.com/bede/hostile) is an alignment-based approach that performs well. It take longer and uses more memory than the `nohuman` kraken approach, but has slightly better accuracy for Illumina data. See the [paper] for more details and for other alternate approaches.

## Cite

[![DOI:10.1093/gigascience/giae010](https://img.shields.io/badge/citation-10.1093/gigascience/giae010-blue)][paper]

> Hall, Michael B., and Lachlan J. M. Coin. “Pangenome databases improve host removal and mycobacteria classification from clinical metagenomic data” GigaScience, April 4, 2024. <https://doi.org/10.1093/gigascience/giae010>

```bibtex
@article{hall_pangenome_2024,
	title = {Pangenome databases improve host removal and mycobacteria classification from clinical metagenomic data},
	volume = {13},
	issn = {2047-217X},
	url = {https://doi.org/10.1093/gigascience/giae010},
	doi = {10.1093/gigascience/giae010},
	urldate = {2024-04-07},
	journal = {GigaScience},
	author = {Hall, Michael B and Coin, Lachlan J M},
	month = jan,
	year = {2024},
	pages = {giae010},
}

```

[quay.io]: https://quay.io/repository/mbhall88/nohuman

[singularity]: https://sylabs.io/guides/3.5/user-guide/quick_start.html#quick-installation-steps

[docker]: https://docs.docker.com/v17.12/install/

[kraken]: https://github.com/DerrickWood/kraken2

[paper]: https://doi.org/10.1093/gigascience/giae010
