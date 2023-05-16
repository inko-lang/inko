FROM fedora-minimal:38 AS builder

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL sparse

# Fedora builds LLVM with libffi support, and when statically linking against
# LLVM the build will fail if libffi-devel isn't installed, hence we include it
# here. See https://gitlab.com/taricorp/llvm-sys.rs/-/issues/41 for some extra
# details.
RUN microdnf install --assumeyes gcc make rust cargo \
    llvm15 llvm15-devel llvm15-static libstdc++-devel libstdc++-static \
    libffi-devel zlib-devel
ADD . /inko/
WORKDIR /inko
RUN make build PREFIX='/usr'
RUN strip target/release/inko
RUN make install PREFIX='/usr'

FROM fedora-minimal:38

# libgcc is needed because libgcc is dynamically linked to the executable.
RUN microdnf install --assumeyes libgcc

COPY --from=builder ["/usr/bin/inko", "/usr/bin/inko"]
COPY --from=builder ["/usr/lib/inko", "/usr/lib/inko/"]
COPY --from=builder ["/usr/share/licenses/inko", "/usr/share/licenses/inko/"]
