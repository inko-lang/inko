CARGO_CMD := rustup run nightly cargo

debug:
	${CARGO_CMD} build

test:
	${CARGO_CMD} test

release:
	${CARGO_CMD} build --release

clean:
	${CARGO_CMD} clean

.PHONY: debug clean
