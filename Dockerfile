FROM registry.fedoraproject.org/fedora-minimal:40 AS builder

# Fedora builds LLVM with libffi support, and when statically linking against
# LLVM the build will fail if libffi-devel isn't installed, hence we include it
# here. See https://gitlab.com/taricorp/llvm-sys.rs/-/issues/41 for some extra
# details.
RUN microdnf install --assumeyes gcc make rust cargo \
    llvm17 llvm17-devel llvm17-static libstdc++-devel libstdc++-static \
    libffi-devel zlib-devel
ADD . /inko/
WORKDIR /inko
RUN make build PREFIX='/usr'
RUN strip target/release/inko
RUN make install PREFIX='/usr'

FROM registry.fedoraproject.org/fedora-minimal:40

# gcc is needed to link object files. This also pulls in libgcc, which the
# generated code links against dynamically.
RUN microdnf install --assumeyes gcc

COPY --from=builder ["/usr/bin/inko", "/usr/bin/inko"]
COPY --from=builder ["/usr/lib/inko", "/usr/lib/inko/"]
COPY --from=builder ["/usr/share/licenses/inko", "/usr/share/licenses/inko/"]
