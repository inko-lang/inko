FROM alpine:3 AS builder

RUN apk add --update make libffi libffi-dev rust cargo build-base
ADD . /inko/
WORKDIR /inko
RUN make build FEATURES=libffi-system PREFIX='/usr'
RUN strip target/release/inko
RUN make install PREFIX='/usr'

FROM alpine:3

# libgcc is needed because libgcc is dynamically linked to the executable.
RUN apk add --update libffi libffi-dev libgcc

COPY --from=builder ["/usr/bin/inko", "/usr/bin/inko"]
COPY --from=builder ["/usr/lib/inko", "/usr/lib/inko/"]
COPY --from=builder ["/usr/share/licenses/inko", "/usr/share/licenses/inko/"]
