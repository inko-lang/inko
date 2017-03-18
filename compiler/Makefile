CARGO_CMD := cargo

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

.PHONY: debug test release profile clean
