FROM alpine:3

ENV LLVM_VERSION 17
ENV RUST_VERSION 1.78

ENV PATH /opt/cargo/bin:$PATH
ENV RUSTUP_HOME /opt/rustup
ENV CARGO_HOME /opt/cargo

RUN apk add --update build-base tar git rustup \
    llvm$LLVM_VERSION llvm$LLVM_VERSION-dev llvm$LLVM_VERSION-static \
    libstdc++-dev zlib-static zstd-static libxml2-static

RUN rustup-init --quiet -y --no-modify-path --profile minimal \
    --component clippy,rustfmt --default-toolchain $RUST_VERSION
