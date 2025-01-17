FROM registry.fedoraproject.org/fedora:41

ENV LLVM_VERSION 17
ENV RUST_VERSION 1.78

ENV PATH /opt/cargo/bin:$PATH
ENV RUSTUP_HOME /opt/rustup
ENV CARGO_HOME /opt/cargo

RUN sudo dnf install --assumeyes --quiet gcc make tar git rustup rclone \
    musl-gcc llvm$LLVM_VERSION llvm$LLVM_VERSION-devel \
    llvm$LLVM_VERSION-static libstdc++-devel libstdc++-static \
    libffi-devel zlib-devel

RUN rustup-init --quiet -y --no-modify-path --profile minimal \
    --component clippy,rustfmt --default-toolchain $RUST_VERSION

# idoc targets the latest stable release of Inko, but our tests run against a
# newer version. This allows us to use an existing idoc executable without
# having to compile it ourselves.
COPY --from=ghcr.io/inko-lang/idoc:latest ["/usr/bin/idoc", "/usr/bin/idoc"]
COPY --from=ghcr.io/inko-lang/idoc:latest ["/usr/share/idoc/assets", "/usr/share/idoc/assets"]
