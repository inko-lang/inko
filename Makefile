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

# The directory the standard library is located at at runtime.
RUNTIME_STD := ${PREFIX}/${LIB}/inko/std

# The directory to place runtime library files in.
INSTALL_RT := ${INSTALL_PREFIX}/${LIB}/inko/runtime/${RUNTIME_NAME}

# The directory the runtime library is located at at runtime.
RUNTIME_RT := ${PREFIX}/${LIB}/inko/runtime

# The install path of the license file.
INSTALL_LICENSE := ${INSTALL_PREFIX}/share/licenses/inko/LICENSE

ifeq (, $(shell command -v cargo 2> /dev/null))
$(warning "The Inko version couldn't be determined, releasing won't be possible")
else
	# The version to build.
	VERSION != cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2
endif

RCLONE_DOCS_TARGET     := /var/lib/shost/docs.inko-lang.org
RCLONE_RELEASES_TARGET := /var/lib/shost/releases.inko-lang.org

# The folder to put the documentation in, allowing for branch specific
# documentation.
DOCS_REF := main

# The directory to store temporary files in.
TMP_DIR := tmp

# The path of the archive to build for source releases.
SOURCE_TAR := ${TMP_DIR}/${VERSION}.tar.gz

# The path to the versions file.
MANIFEST_NAME := manifest.txt
MANIFEST := ${TMP_DIR}/${MANIFEST_NAME}

build:
	INKO_STD=${RUNTIME_STD} INKO_RT=${RUNTIME_RT} cargo build --release

${TMP_DIR}:
	mkdir -p "${@}"

${SOURCE_TAR}: ${TMP_DIR}
	git archive --format tar HEAD \
		.cargo \
		CHANGELOG.md \
		Cargo.lock \
		Cargo.toml \
		LICENSE \
		Makefile \
		ast \
		compiler \
		inko \
		std/src \
		rt \
		types \
		location \
		| gzip > "${@}"

release/source: ${SOURCE_TAR}
	scripts/rclone.sh copy "${SOURCE_TAR}" ":sftp:${RCLONE_RELEASES_TARGET}"

release/manifest: ${TMP_DIR}
	scripts/rclone.sh copyto \
		":sftp:${RCLONE_RELEASES_TARGET}/${MANIFEST_NAME}" "${MANIFEST}"
	echo "${VERSION}" >> "${MANIFEST}"
	sort --version-sort "${MANIFEST}"
	scripts/rclone.sh copy "${MANIFEST}" ":sftp:${RCLONE_RELEASES_TARGET}"

release/changelog:
	clogs "${VERSION}"

release/versions:
	scripts/update_versions.sh ${VERSION}

release/commit:
	git add */Cargo.toml Cargo.toml Cargo.lock CHANGELOG.md
	git commit -m "Release v${VERSION}"
	git push origin "$$(git rev-parse --abbrev-ref HEAD)"

release/tag:
	git tag -a -m "Release v${VERSION}" "v${VERSION}"
	git push origin "v${VERSION}"

release/publish: release/versions release/changelog release/commit release/tag

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
	rm -rf "${TMP_DIR}"
	rm -rf build
	rm -rf std/build
	rm -rf docs/public
	cargo clean

docs/setup:
	cd docs && inko pkg sync

docs/build:
	rm -rf docs/public
	cd docs && inko build --release && DOCS_REF=${DOCS_REF} ./build/release/main

docs/watch:
	cd docs && DOCS_REF=${DOCS_REF} ./scripts/watch.sh

docs/publish: docs/setup docs/build
	scripts/rclone.sh sync docs/public ":sftp:${RCLONE_DOCS_TARGET}/manual/${DOCS_REF}"

std-docs/build:
	rm -rf std/build
	cargo build
	cd std && idoc --compiler ../target/debug/inko

std-docs/publish: std-docs/build
	scripts/rclone.sh sync std/build/idoc/public ":sftp:${RCLONE_DOCS_TARGET}/std/${DOCS_REF}"

.PHONY: release/source release/manifest release/changelog release/versions
.PHONY: release/commit release/publish release/tag
.PHONY: build install clean
.PHONY: docs/setup docs/build docs/watch docs/publish
.PHONY: std-docs/build std-docs/publish
