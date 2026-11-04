# The directory to move files into as part of the installation procedure.
DESTDIR :=

# The base directory for executables, loading compiler source files, etc.
# This path _must_ start with a /.
PREFIX := /usr

# The name of the library directory, usually "lib" or "lib64"
LIB := lib

ifneq (${DESTDIR},)
	INSTALL_PREFIX = ${DESTDIR}${PREFIX}
else
	INSTALL_PREFIX = ${PREFIX}
endif

# The name of the static runtime library.
RUNTIME_NAME := libinko.a

# The directory to place the Inko executable in.
INSTALL_INKO := ${INSTALL_PREFIX}/bin/inko

# The directory to place the standard library in.
INSTALL_STD := ${INSTALL_PREFIX}/${LIB}/inko/std

# The directory to place runtime library files in.
INSTALL_RT := ${INSTALL_PREFIX}/${LIB}/inko/runtime/${RUNTIME_NAME}

# The install path of the license file.
INSTALL_LICENSE := ${INSTALL_PREFIX}/share/licenses/inko/LICENSE

build:
	cargo build --release

${INSTALL_STD}:
	mkdir -p "${@}"
	cp -r std/src/* "${@}"

${INSTALL_RT}:
	mkdir -p "$$(dirname ${@})"
	install -m644 target/release/${RUNTIME_NAME} "${@}"

${INSTALL_INKO}:
	mkdir -p "$$(dirname ${@})"
	install -m755 target/release/inko "${@}"

${INSTALL_LICENSE}:
	mkdir -p "$$(dirname ${@})"
	install -m644 LICENSE "${@}"

install: ${INSTALL_STD} \
	${INSTALL_RT} \
	${INSTALL_INKO} \
	${INSTALL_LICENSE}

clean:
	cargo clean

.PHONY: build install clean
