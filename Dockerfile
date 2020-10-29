FROM alpine:3 AS builder

RUN apk add --update make libffi libffi-dev rust cargo build-base

# Per https://github.com/containers/buildah/issues/1849#issuecomment-635579332,
# the presence of a .dockerignore slows down the build process. To work around
# this, we add the necessary files/directories explicitly, instead of using
# `ADD . /inko/` to add files.
ADD Cargo.lock Cargo.toml LICENSE Makefile /inko/
ADD .cargo /inko/.cargo/
ADD cli /inko/cli/
ADD compiler /inko/compiler/
ADD runtime /inko/runtime/
ADD vm /inko/vm/

WORKDIR /inko
RUN make build FEATURES='libinko/libffi-system' PREFIX='/usr'
RUN strip target/release/inko
RUN make install PREFIX='/usr'

FROM alpine:3

# libgcc is needed because libgcc is dynamically linked to the executable.
RUN apk add --update libffi libffi-dev ruby ruby-json libgcc

COPY --from=builder ["/usr/bin/inko", "/usr/bin/inko"]
COPY --from=builder ["/usr/lib/inko", "/usr/lib/inko/"]
COPY --from=builder ["/usr/share/licenses/inko", "/usr/share/licenses/inko/"]

# Disable any warnings about the Ruby compiler.
ENV RUBYOPT '-W:no-deprecated -W:no-experimental'
