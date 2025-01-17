FROM debian:trixie

ENV LLVM_VERSION 17
ENV RUST_VERSION 1.78

ENV PATH /opt/cargo/bin:$PATH
ENV CARGO_HOME /opt/cargo
ENV RUSTUP_HOME /opt/rustup

RUN apt-get update --quiet && apt-get install --quiet --yes \
    musl-tools curl build-essential tar git rustup llvm-$LLVM_VERSION \
    llvm-$LLVM_VERSION-dev libstdc++-12-dev libclang-common-$LLVM_VERSION-dev \
    zlib1g-dev libpolly-$LLVM_VERSION-dev libzstd-dev

RUN rustup toolchain install $RUST_VERSION \
    --profile minimal --component clippy,rustfmt && rustup default $RUST_VERSION
