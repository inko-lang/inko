FROM debian:12

ENV LLVM_VERSION 17
ENV RUST_VERSION 1.78

ENV PATH /opt/cargo/bin:$PATH
ENV CARGO_HOME /opt/cargo
ENV RUSTUP_HOME /opt/rustup

RUN apt update --quiet && apt install --quiet --yes curl build-essential tar git
RUN curl https://apt.llvm.org/llvm-snapshot.gpg.key | tee /etc/apt/trusted.gpg.d/apt.llvm.org.asc
RUN /usr/bin/echo -e "deb http://apt.llvm.org/bookworm/ llvm-toolchain-bookworm-$LLVM_VERSION main" \
    > /etc/apt/sources.list.d/llvm$LLVM_VERSION.list

RUN apt update --quiet && apt install --quiet --yes \
    llvm-$LLVM_VERSION llvm-$LLVM_VERSION-dev \
    libstdc++-11-dev libclang-common-$LLVM_VERSION-dev zlib1g-dev \
    libpolly-$LLVM_VERSION-dev libzstd-dev

RUN curl --proto '=https' --tlsv1.2 --retry 10 --retry-connrefused --location \
    --silent --show-error --fail "https://sh.rustup.rs" | \
    sh -s -- --quiet -y --no-modify-path --profile minimal \
    --component clippy,rustfmt --default-toolchain $RUST_VERSION

RUN dpkg --add-architecture arm64 \
    && apt update --quiet \
    && apt install --quiet --yes gcc-aarch64-linux-gnu musl-tools:arm64
