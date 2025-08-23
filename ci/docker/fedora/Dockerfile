FROM registry.fedoraproject.org/fedora:42

ENV LLVM_VERSION 18
ENV RUST_VERSION 1.89
env MIN_RUST_VERSION 1.78

ENV PATH /opt/cargo/bin:$PATH
ENV RUSTUP_HOME /opt/rustup
ENV CARGO_HOME /opt/cargo

# This is set such that when we compile the runtimes for ARM64 we just use the
# host musl-gcc, which is for ARM64 anyway (since the build process runs on an
# ARM64 host).
ENV CC_aarch64_unknown_linux_musl=musl-gcc

RUN sudo dnf install --assumeyes --quiet gcc make tar git rustup rclone \
    musl-gcc llvm$LLVM_VERSION llvm$LLVM_VERSION-devel \
    llvm$LLVM_VERSION-static libstdc++-devel libstdc++-static \
    libffi-devel zlib-devel

RUN rustup-init --quiet -y --no-modify-path --profile minimal \
    --component clippy,rustfmt --default-toolchain $RUST_VERSION \
    && rustup toolchain install $MIN_RUST_VERSION

# idoc targets the latest stable release of Inko, but our tests run against a
# newer version. This allows us to use an existing idoc executable without
# having to compile it ourselves.
COPY --from=ghcr.io/inko-lang/idoc:latest ["/usr/bin/idoc", "/usr/bin/idoc"]
COPY --from=ghcr.io/inko-lang/idoc:latest ["/usr/share/idoc/assets", "/usr/share/idoc/assets"]
