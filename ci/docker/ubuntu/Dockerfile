FROM ubuntu:24.04

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL sparse
ENV LLVM_VERSION 17
ENV RUST_VERSION 1.70

ENV PATH /opt/cargo/bin:$PATH
ENV CARGO_HOME /opt/cargo
ENV RUSTUP_HOME /opt/rustup

RUN apt-get update --quiet && apt-get install --quiet --yes \
    build-essential tar git curl \
    llvm-$LLVM_VERSION llvm-$LLVM_VERSION-dev \
    libstdc++-11-dev libclang-common-$LLVM_VERSION-dev zlib1g-dev \
    libpolly-16-dev libzstd-dev

RUN curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location \
    --silent --show-error --fail "https://sh.rustup.rs" | \
    sh -s -- --quiet -y --no-modify-path --profile minimal \
    --component clippy,rustfmt --default-toolchain $RUST_VERSION
