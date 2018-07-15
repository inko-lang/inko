# The version of the runtime to install. This version will be part of the
# library path, unless a custom LIBDIR is specified.
VERSION != cat LANGUAGE_VERSION

# The base directory to install the runtime in. Typically this will be either
# /usr or ~/.local.
PREFIX := /usr

# The directory to copy the runtime/ directory to.
LIBDIR := ${PREFIX}/lib/inko/${VERSION}

install:
	mkdir -p "${LIBDIR}/runtime"
	cp -r src/* "${LIBDIR}/runtime"

uninstall:
	rm -rf "${LIBDIR}"

.PHONY: install uninstall
