# The cargo command to use for building the VM.
CARGO_CMD := cargo

# The base directory to install the VM in. Typically this will be either /usr or
# ~/.local.
PREFIX := /usr

# The directory to copy the VM executable to.
BINDIR := ${PREFIX}/bin

debug:
	CARGO_INCREMENTAL=1 ${CARGO_CMD} build

check:
	CARGO_INCREMENTAL=1 ${CARGO_CMD} check

test:
	CARGO_INCREMENTAL=1 ${CARGO_CMD} test

release:
	${CARGO_CMD} build --release

profile:
	RUSTFLAGS="-g" ${CARGO_CMD} build --release

clean:
	${CARGO_CMD} clean

install:
	${CARGO_CMD} build --release
	install -Dm755 target/release/ivm "${BINDIR}/ivm"

uninstall:
	rm -f "${BINDIR}/ivm"

.PHONY: debug test release profile clean install
