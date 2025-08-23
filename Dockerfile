FROM registry.fedoraproject.org/fedora-minimal:42 AS builder

# Fedora builds LLVM with libffi support, and when statically linking against
# LLVM the build will fail if libffi-devel isn't installed, hence we include it
# here. See https://gitlab.com/taricorp/llvm-sys.rs/-/issues/41 for some extra
# details.
RUN microdnf install --assumeyes gcc make rust cargo \
    llvm18 llvm18-devel llvm18-static libstdc++-devel libstdc++-static \
    libffi-devel zlib-devel
ADD . /inko/
WORKDIR /inko
RUN make build PREFIX='/usr'
RUN strip target/release/inko
RUN make install PREFIX='/usr'

FROM registry.fedoraproject.org/fedora-minimal:42

# gcc is needed to link object files. This also pulls in libgcc, which the
# generated code links against dynamically.
#
# We also install tar and Git such that GitHub Actions jobs can use this image
# without having to install these packages themselves.
RUN microdnf install --assumeyes gcc tar git

COPY --from=builder ["/usr/bin/inko", "/usr/bin/inko"]
COPY --from=builder ["/usr/lib/inko", "/usr/lib/inko/"]
COPY --from=builder ["/usr/share/licenses/inko", "/usr/share/licenses/inko/"]
