CARGO_CMD := rustup run nightly cargo
INCREMENTAL := rustc-incremental
RUSTFLAGS := -Zincremental=${INCREMENTAL}

export RUSTFLAGS

debug:
	${CARGO_CMD} build

test:
	${CARGO_CMD} test

release:
	${CARGO_CMD} build --release

profile:
	RUSTFLAGS="${RUSTFLAGS} -g" ${CARGO_CMD} build --release

clean:
	${CARGO_CMD} clean
	rm -rf ${INCREMENTAL}

.PHONY: debug clean
