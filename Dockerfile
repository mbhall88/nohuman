FROM rust:slim AS builder

COPY . /nohuman

WORKDIR /nohuman

RUN apt update \
    && apt install -y musl-tools libssl-dev pkg-config \
    && cargo build --release \
    && strip target/release/nohuman

FROM ubuntu:24.04 as kraken

# for easy upgrade later. ARG variables only persist during build time.
ARG K2VER="2.17"

# install dependencies and cleanup apt garbage
RUN apt-get update && apt-get -y --no-install-recommends install \
    wget \
    ca-certificates \
    zlib1g-dev \
    make \
    g++ \
    libgoogle-perftools-dev \
    rsync \
    cpanminus \
    ncbi-blast+ && \
    rm -rf /var/lib/apt/lists/* && apt-get autoclean

# perl module required for kraken2-build
RUN cpanm Getopt::Std

# DL Kraken2, unpack, and install
RUN wget https://github.com/DerrickWood/kraken2/archive/v${K2VER}.tar.gz && \
    tar -xzf v${K2VER}.tar.gz && \
    rm -rf v${K2VER}.tar.gz && \
    cd kraken2-${K2VER} && \
    ./install_kraken2.sh /bin


FROM ubuntu:24.04

COPY --from=builder /nohuman/target/release/nohuman /bin/
COPY --from=kraken /bin/kraken2* /bin/

RUN nohuman --version && \
    nohuman --check
# print help and versions
RUN kraken2 --help && \
    kraken2-build --help && \
    kraken2 --version


ENTRYPOINT [ "nohuman" ]
