# The location to store the built Gem in.
PREFIX := /usr

# The location to install the Ruby code into.
LIBDIR := ${PREFIX}/lib/inko

# The location to store the executable files in.
BINDIR := ${PREFIX}/bin

# The location to copy the compiler code to when building.
COMPILER_DIR := ${PREFIX}/compiler

test:
	bundle exec rspec spec

build:
	mkdir -p "${COMPILER_DIR}"
	cp -r bin "${COMPILER_DIR}"
	cp -r lib "${COMPILER_DIR}"
	cp README.md "${COMPILER_DIR}"
	cp Makefile "${COMPILER_DIR}"
	cp install.rb "${COMPILER_DIR}"

install:
	env LIBDIR="${LIBDIR}" BINDIR="${BINDIR}" ruby install.rb

uninstall:
	rm -f "${BINDIR}/inko"
	rm -f "${BINDIR}/inko-test"
	rm -f "${BINDIR}/inkoc"
	rm -rf "${LIBDIR}/compiler"

.PHONY: test install uninstall
