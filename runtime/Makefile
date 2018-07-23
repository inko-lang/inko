# The base directory to install the runtime in. Typically this will be either
# /usr or ~/.local.
PREFIX := /usr

# The directory to copy the runtime/ directory to.
LIBDIR := ${PREFIX}/lib/inko

install:
	mkdir -p "${LIBDIR}/runtime"
	cp -r src/* "${LIBDIR}/runtime"

uninstall:
	rm -rf "${LIBDIR}"

.PHONY: install uninstall
