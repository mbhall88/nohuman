# stage: build the Rust binary
FROM rust:slim AS builder

COPY . /nohuman
WORKDIR /nohuman

RUN apt update \
    && apt install -y musl-tools libssl-dev pkg-config \
    && cargo build --release \
    && strip target/release/nohuman

# stage: build and patch Kraken2
FROM ubuntu:24.04 AS kraken

ARG K2VER="2.17"

# install dependencies (include git so we can apply patches)
RUN apt-get update && apt-get -y --no-install-recommends install \
    wget \
    ca-certificates \
    zlib1g-dev \
    make \
    g++ \
    libgoogle-perftools-dev \
    rsync \
    cpanminus \
    ncbi-blast+ \
    git \
    && rm -rf /var/lib/apt/lists/* && apt-get autoclean

# perl module required for kraken2-build
RUN cpanm Getopt::Std

# Download Kraken2, apply patches (both Makefile+install_kraken2.sh and scripts/k2),
# then install into the container. We initialize a temporary git repo to allow git apply.
RUN wget https://github.com/DerrickWood/kraken2/archive/v${K2VER}.tar.gz \
    && tar -xzf v${K2VER}.tar.gz \
    && rm -rf v${K2VER}.tar.gz \
    && cd kraken2-${K2VER} \
    && git init . \
    && git add . \
    && git commit -m "orig kraken2" >/dev/null 2>&1 || true \
    && cat > /tmp/0001-src-Makefile.patch <<'PATCH'
diff --git a/src/Makefile b/src/Makefile
index df902f3..39be19e 100644
--- a/src/Makefile
+++ b/src/Makefile
@@ -1,17 +1,17 @@
-CXX ?= g++
+CXX ?= $(CXX)
 KRAKEN2_SKIP_FOPENMP ?= -fopenmp
-CXXFLAGS = $(KRAKEN2_SKIP_FOPENMP) -Wall -std=c++11 -O3 -fPIC
-CXXFLAGS += -DLINEAR_PROBING
-CFLAGS = -Wall -std=c99 -O0 -g3
+CXXFLAGS = $(KRAKEN2_SKIP_FOPENMP) -Wall -std=c++14 -O3 -fPIC
+CXXFLAGS += -DLINEAR_PROBING $(LDFLAGS)
+CFLAGS = -Wall -std=c11 -O3 -g3
 
 .PHONY: all clean install
 
-PROGS = estimate_capacity build_db classify dump_table lookup_accession_numbers k2mask blast_to_fasta libtax
+PROGS = estimate_capacity build_db classify dump_table lookup_accession_numbers k2mask blast_to_fasta libtax.so
 
 all: $(PROGS)
 
 install: $(PROGS)
-	cp $(PROGS) "$(KRAKEN2_DIR)/"
+	install -v -m 0755 $(PROGS) "$(KRAKEN2_DIR)"
 
 clean:
 	rm -f *.o $(PROGS)
@@ -61,5 +61,5 @@ blast_to_fasta: blast_to_fasta.o blast_defline.o blast_utils.o
 
 libtax.o: libtax.cc
 
-libtax: taxonomy.o mmap_file.o libtax.o
+libtax.so: taxonomy.o mmap_file.o libtax.o
 	$(CXX) $(CXXFLAGS) -shared -o libtax.so taxonomy.o mmap_file.o libtax.o
diff --git a/install_kraken2.sh b/install_kraken2.sh
index 248b553..5a9a7eb 100755
--- a/install_kraken2.sh
+++ b/install_kraken2.sh
@@ -28,10 +28,10 @@ fi
 # Perl cmd used to canonicalize dirname - "readlink -f" doesn't work
 # on OS X.
 export KRAKEN2_DIR
-KRAKEN2_DIR=$(perl -MCwd=abs_path -le 'print abs_path(shift)' "$1")
+KRAKEN2_DIR="${PREFIX}/share/${PKG_NAME}-${PKG_VERSION}/libexec"
 
 mkdir -p "$KRAKEN2_DIR"
-make -C src install
+make -C src install CXX="$CXX" CC="$CC" -j"$CPU_COUNT"
 for file in scripts/*
 do
   destination_file="$KRAKEN2_DIR/$(basename "$file")"
PATCH
    && cat > /tmp/0002-k2.patch <<'PATCH'
diff --git a/scripts/k2 b/scripts/k2
old mode 100755
new mode 100644
index 04160c2..7d5a574
--- a/scripts/k2
+++ b/scripts/k2
@@ -431,7 +431,7 @@ def download_and_process_blast_volumes(args):
     ) as pool:
         with open("manifest.txt", "r") as in_file:
             tarballs = in_file.readlines()
-        f = functools.partial(
+        wrapped_func = functools.partial(
             wrap_with_globals, extract_blast_db_files,
             LOG.get_queue(), LOG.get_level(),
             SCRIPT_PATHNAME
@@ -442,7 +442,7 @@ def download_and_process_blast_volumes(args):
         )
         for tarball in tarballs:
             tarball = os.path.abspath(tarball)
-            f = pool.submit(extract_blast_db_files, tarball.strip())
+            f = pool.submit(wrapped_func, tarball.strip())
             extraction_futures.append(f)
         for future in concurrent.futures.as_completed(extraction_futures):
             result = future.result()
@@ -4214,12 +4214,17 @@ def merge_classification_output_parallel(
         input = list(zip(file1.readlines(), file2.readlines()))
     input_len = len(input)
     partition_ranges = list(range(0, input_len, int(input_len / args.threads)))
-    partition_ranges[-1] = input_len
+    partition_ranges.append(input_len)
     job_number = 0
     futures = []
+    wrapped_func = functools.partial(
+        wrap_with_globals, merge_classification_output2,
+        LOG.get_queue(), LOG.get_level(),
+        SCRIPT_PATHNAME
+    )
     for start, end in zip(partition_ranges, partition_ranges[1:]):
         future = pool.submit(
-            merge_classification_output2, taxonomy_pathname,
+            wrapped_func, taxonomy_pathname,
             input[start:end], job_number, use_names, args,
             save_seq_names, final
         )
PATCH
    && git apply /tmp/0001-src-Makefile.patch \
    && git apply /tmp/0002-k2.patch \
    && ./install_kraken2.sh /bin \
    && cp -v scripts/* /bin/ 2>/dev/null || true \
    && cp -v src/estimate_capacity src/build_db src/classify src/dump_table src/lookup_accession_numbers src/k2mask src/blast_to_fasta /bin/ 2>/dev/null || true \
    && test -f src/libtax.so && cp -v src/libtax.so /usr/lib/ 2>/dev/null || true

# final image
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