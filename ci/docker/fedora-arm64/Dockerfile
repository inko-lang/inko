FROM registry.fedoraproject.org/fedora:44

ENV LLVM_VERSION=18
ENV RUST_VERSION=1.94
ENV MIN_RUST_VERSION=1.85

ENV PATH=/opt/cargo/bin:$PATH
ENV RUSTUP_HOME=/opt/rustup
ENV CARGO_HOME=/opt/cargo

# This is set such that when we compile the runtimes for ARM64 we just use the
# host musl-gcc, which is for ARM64 anyway (since the build process runs on an
# ARM64 host).
ENV CC_aarch64_unknown_linux_musl=musl-gcc

RUN sudo dnf install --assumeyes --quiet gcc make tar git rustup \
    musl-gcc llvm$LLVM_VERSION llvm$LLVM_VERSION-devel \
    llvm$LLVM_VERSION-static libstdc++-devel libstdc++-static \
    libffi-devel zlib-devel

RUN rustup-init --quiet -y --no-modify-path --profile minimal \
    --component clippy,rustfmt --default-toolchain $RUST_VERSION \
    && rustup toolchain install $MIN_RUST_VERSION
