# The location to store the built Gem in.
PREFIX := /usr

# The location to install the Ruby code into.
LIBDIR := ${PREFIX}/lib/inko

# The location to store the executable files in.
BINDIR := ${PREFIX}/bin

test:
	bundle exec rspec spec

install:
	mkdir -p "${LIBDIR}" "${BINDIR}"
	cp -r lib "${LIBDIR}/compiler"
	install -m755 bin/* "${BINDIR}"

uninstall:
	rm -f "${BINDIR}/inko"
	rm -f "${BINDIR}/inko-test"
	rm -f "${BINDIR}/inkoc"
	rm -rf "${LIBDIR}/compiler"

.PHONY: test install uninstall
