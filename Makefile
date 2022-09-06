# The directory to move files into as part of the installation procedure.
DESTDIR :=

# The base directory for executables, loading compiler source files, etc.
# This path _must_ start with a /.
PREFIX := /usr

ifneq (${DESTDIR},)
	INSTALL_PREFIX = ${DESTDIR}${PREFIX}
else
	INSTALL_PREFIX = ${PREFIX}
endif

# The directory to place the executable in.
INSTALL_BIN := ${INSTALL_PREFIX}/bin/inko

# The directory to place the standard library in.
INSTALL_STD := ${INSTALL_PREFIX}/lib/inko/libstd

# The install path of the license file.
INSTALL_LICENSE := ${INSTALL_PREFIX}/share/licenses/inko/LICENSE

# The target to use for cross compilation. An empty string indicates the default
# target of the underlying platform.
TARGET :=

# The list of features to enable when building the VM.
FEATURES :=

ifneq (${TARGET},)
	TARGET_OPTION=--target ${TARGET}
	TARGET_BINARY=target/${TARGET}/release/inko
else
	TARGET_OPTION=
	TARGET_BINARY=target/release/inko
endif

ifneq (${FEATURES},)
	FEATURES_OPTION=--features ${FEATURES}
else
	FEATURES_OPTION=
endif

ifeq (, $(shell command -v cargo 2> /dev/null))
$(warning "The Inko version couldn't be determined, releasing won't be possible")
else
	# The version to build.
	VERSION != cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2
endif

# The name of the S3 bucket that contains all releases.
RELEASES_S3_BUCKET := releases.inko-lang.org

# The ID of the cloudfront distribution that serves all packages.
RELEASES_CLOUDFRONT_ID := E3SFQ1OG1H5PCN

# The name of the S3 bucket for uploading documentation.
DOCS_S3_BUCKET := docs.inko-lang.org

# The ID of the cloudfront distribution that serves the documentation.
DOCS_CLOUDFRONT_ID := E3S16BR117BJOL

# The folder to put the documentation in, allowing for branch specific
# documentation.
DOCS_FOLDER := master

# The directory to store temporary files in.
TMP_DIR := tmp

# The path of the archive to build for source releases.
SOURCE_TAR := ${TMP_DIR}/${VERSION}.tar.gz

# The path to the versions file.
MANIFEST := ${TMP_DIR}/manifest.txt

build:
	INKO_LIBSTD=${INSTALL_STD} cargo build --release

${TMP_DIR}:
	mkdir -p "${@}"

${SOURCE_TAR}: ${TMP_DIR}
	git archive --format tar HEAD \
		ast \
		bytecode \
		compiler \
		libstd/src \
		vm \
		.cargo \
		Cargo.toml \
		Cargo.lock \
		CHANGELOG.md \
		LICENSE \
		Makefile \
		| gzip > "${@}"

release/source: ${SOURCE_TAR}
	aws s3 cp --acl public-read "${SOURCE_TAR}" s3://${RELEASES_S3_BUCKET}/

release/manifest: ${TMP_DIR}
	aws s3 ls s3://${RELEASES_S3_BUCKET}/ | \
		grep -oP '(\d+\.\d+\.\d+\.tar.gz)$$' | \
		grep -oP '(\d+\.\d+\.\d+)' | \
		sort > "${MANIFEST}"
	aws s3 cp --acl public-read "${MANIFEST}" s3://${RELEASES_S3_BUCKET}/
	aws cloudfront create-invalidation \
		--distribution-id ${RELEASES_CLOUDFRONT_ID} --paths "/*"

release/changelog:
	ruby scripts/changelog.rb "${VERSION}"

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
	cp -r libstd/src/* "${@}"

${INSTALL_BIN}:
	mkdir -p "$$(dirname ${@})"
	install -m755 ${TARGET_BINARY} "${@}"

${INSTALL_LICENSE}:
	mkdir -p "$$(dirname ${@})"
	install -m644 LICENSE "${@}"

install: ${INSTALL_STD} \
	${INSTALL_BIN} \
	${INSTALL_LICENSE}

uninstall:
	rm -rf ${INSTALL_STD}
	rm -f ${INSTALL_BIN}
	rm -rf ${INSTALL_PREFIX}/share/licenses/inko

clean:
	rm -rf "${TMP_DIR}"
	cargo clean

docs/install:
	cd docs && poetry install

docs/build:
	cd docs && poetry run mkdocs build

docs/server:
	cd docs && poetry run mkdocs serve

docs/publish: docs/install docs/build
	aws s3 sync docs/build s3://${DOCS_S3_BUCKET}/manual/${DOCS_FOLDER} \
		--acl=public-read --delete --cache-control max-age=86400
	aws cloudfront create-invalidation \
		--distribution-id ${DOCS_CLOUDFRONT_ID} --paths "/*"

clippy:
	touch */src/lib.rs */src/main.rs
	cargo clippy ${TARGET_OPTION} ${FEATURES_OPTION}

rustfmt-check:
	rustfmt --check */src/lib.rs */src/main.rs

rustfmt:
	rustfmt --emit files */src/lib.rs */src/main.rs

.PHONY: release/source release/manifest release/changelog release/versions
.PHONY: release/commit release/publish release/tag
.PHONY: build install uninstall clean
.PHONY: libstd/test rustfmt rustfmt-check clippy
.PHONY: docs/install docs/build docs/server docs/publish
