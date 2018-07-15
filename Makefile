# The base directory to install the runtime in. Typically this will be either
# /usr or ~/.local.
PREFIX := /usr

install:
	(cd compiler && make install)
	(cd runtime && make install PREFIX="${PREFIX}")
	(cd vm && make install PREFIX="${PREFIX}")

uninstall:
	(cd compiler && make uninstall)
	(cd runtime && make uninstall PREFIX="${PREFIX}")
	(cd vm && make uninstall PREFIX="${PREFIX}")

.PHONY: install uninstall
