FROM registry.fedoraproject.org/fedora:40

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL sparse
ENV LLVM_VERSION 17
ENV RUST_VERSION 1.70

ENV PATH /opt/cargo/bin:$PATH
ENV CARGO_HOME /opt/cargo
ENV RUSTUP_HOME /opt/rustup

RUN sudo dnf install --assumeyes --quiet gcc make tar git rustup rclone \
    llvm$LLVM_VERSION llvm$LLVM_VERSION-devel \
    llvm$LLVM_VERSION-static libstdc++-devel libstdc++-static \
    libffi-devel zlib-devel

RUN rustup-init --quiet -y --no-modify-path --profile minimal \
    --component clippy,rustfmt --default-toolchain $RUST_VERSION
