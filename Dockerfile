FROM rust:slim AS builder

COPY . /nohuman

WORKDIR /nohuman

RUN apt update \
    && apt install -y musl-tools libssl-dev pkg-config \
    && cargo build --release \
    && strip target/release/nohuman

FROM ubuntu:jammy as kraken

# for easy upgrade later. ARG variables only persist during build time.
ARG K2VER="2.1.3"

LABEL base.image="ubuntu:jammy"
LABEL dockerfile.version="2"
LABEL software="Kraken2"
LABEL software.version="${K2VER}"
LABEL description="Taxonomic sequence classifier"
LABEL website="https://github.com/DerrickWood/kraken2"
LABEL license="https://github.com/DerrickWood/kraken2/blob/master/LICENSE"
LABEL maintainer="Curtis Kapsak"
LABEL maintainer.email="kapsakcj@gmail.com"
LABEL maintainer2="Erin Young"
LABEL maintainer2.email="eriny@utah.gov"

# install dependencies and cleanup apt garbage
RUN apt-get update && apt-get -y --no-install-recommends install \
    wget \
    ca-certificates \
    zlib1g-dev \
    make \
    g++ \
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


FROM ubuntu:jammy

COPY --from=builder /nohuman/target/release/nohuman /bin/
COPY --from=kraken /bin/kraken2* /bin/

RUN nohuman --version
# print help and versions
RUN kraken2 --help && \
    kraken2-build --help && \
    kraken2 --version


ENTRYPOINT [ "nohuman" ]
