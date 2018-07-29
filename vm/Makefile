# The cargo command to use for building the VM.
CARGO_CMD := cargo

# The base directory to install the VM in. Typically this will be either /usr or
# ~/.local.
PREFIX := /usr

# The directory to copy the VM executable to.
BINDIR := ${PREFIX}/bin

# The target to use for cross compilation. An empty string indicates the default
# target of the underlying platform.
TARGET :=

ifneq (${TARGET},)
	TARGET_OPTION=--target ${TARGET}
else
	TARGET_OPTION=
endif

debug:
	CARGO_INCREMENTAL=1 ${CARGO_CMD} build ${TARGET_OPTION}

check:
	CARGO_INCREMENTAL=1 ${CARGO_CMD} check

test:
	CARGO_INCREMENTAL=1 ${CARGO_CMD} test

release:
	${CARGO_CMD} build --release ${TARGET_OPTION}

profile:
	RUSTFLAGS="-g" ${CARGO_CMD} build --release ${TARGET_OPTION}

clean:
	${CARGO_CMD} clean

install:
	${CARGO_CMD} build --release ${TARGET_OPTION}
	mkdir -p "${BINDIR}"
	install -m755 target/release/ivm "${BINDIR}/ivm"

uninstall:
	rm -f "${BINDIR}/ivm"

.PHONY: debug test release profile clean install
