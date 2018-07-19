# The base directory to install the runtime in. Typically this will be either
# /usr or ~/.local.
PREFIX := /usr

# The version to use for building a source tarball.
VERSION != cat VERSION

# The architecture to use for building the VM.
ARCH != uname -m

install:
	(cd compiler && make install)
	(cd runtime && make install PREFIX="${PREFIX}")
	(cd vm && make install PREFIX="${PREFIX}")

uninstall:
	(cd compiler && make uninstall)
	(cd runtime && make uninstall PREFIX="${PREFIX}")
	(cd vm && make uninstall PREFIX="${PREFIX}")

release:
	rm -rf ./target
	mkdir -p target
	(cd compiler && make build PREFIX=../target)
	(cd runtime && make install PREFIX=../target)
	(cd vm && make install PREFIX=../target)
	cp VERSION target/
	tar --directory target --create --gzip \
		--file inko-${VERSION}-${ARCH}.tar.gz .

.PHONY: install uninstall release
