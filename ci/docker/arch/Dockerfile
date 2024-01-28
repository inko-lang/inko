FROM archlinux:latest

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL sparse
ENV LLVM_VERSION 15
ENV RUST_VERSION 1.70

ENV PATH /opt/cargo/bin:$PATH
ENV CARGO_HOME /opt/cargo
ENV RUSTUP_HOME /opt/rustup

RUN pacman-key --init && \
    pacman-key --populate && \
    pacman -Sy --noconfirm archlinux-keyring && \
    pacman -Syu --noconfirm rustup tar git base-devel curl llvm15

RUN rustup toolchain install --profile minimal $RUST_VERSION && \
    rustup default $RUST_VERSION
