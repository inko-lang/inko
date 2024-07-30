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

# The name of the Cloudflare bucket that contains all releases.
RELEASES_BUCKET := inko-releases

# The name of the Cloudflare bucket for uploading documentation.
DOCS_BUCKET := inko-docs

# The ID of the cloudfront distribution that serves the documentation.
DOCS_CLOUDFRONT_ID := E3S16BR117BJOL

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
		| gzip > "${@}"

release/source: ${SOURCE_TAR}
	rclone copy --config rclone.conf --checksum --verbose \
		"${SOURCE_TAR}" "production:${RELEASES_BUCKET}"

release/manifest: ${TMP_DIR}
	rclone copyto --config rclone.conf \
		"production:${RELEASES_BUCKET}/${MANIFEST_NAME}" "${MANIFEST}"
	echo "${VERSION}" >> "${MANIFEST}"
	sort --version-sort "${MANIFEST}"
	rclone copy --config rclone.conf --checksum --verbose \
		"${MANIFEST}" "production:${RELEASES_BUCKET}"

release/changelog:
	clogs "${VERSION}"

release/versions:
	ruby scripts/update_versions.rb ${VERSION}

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
	cd docs && inko build && DOCS_REF=${DOCS_REF} ./build/main

docs/watch:
	cd docs && DOCS_REF=${DOCS_REF} ./scripts/watch.sh

docs/publish: docs/setup docs/build
	rclone sync --config rclone.conf --checksum --verbose \
		docs/public "production:${DOCS_BUCKET}/manual/${DOCS_REF}"

std-docs/build:
	rm -rf std/build
	cargo build
	cd std && idoc --compiler ../target/debug/inko

std-docs/publish: std-docs/build
	rclone sync --config rclone.conf --checksum --verbose \
		std/build/idoc/public "production:${DOCS_BUCKET}/std/${DOCS_REF}"

runtimes:
	bash scripts/runtimes.sh ${VERSION}

.PHONY: release/source release/manifest release/changelog release/versions
.PHONY: release/commit release/publish release/tag
.PHONY: build install clean runtimes
.PHONY: docs/setup docs/build docs/watch docs/publish
.PHONY: std-docs/build std-docs/publish
