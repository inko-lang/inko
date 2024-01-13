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

# The name of the static runtime library.
RUNTIME_NAME := libinko.a

# The directory to place the Inko executable in.
INSTALL_INKO := ${INSTALL_PREFIX}/bin/inko

# The directory to place the standard library in.
INSTALL_STD := ${INSTALL_PREFIX}/lib/inko/std

# The directory the standard library is located at at runtime.
RUNTIME_STD := ${PREFIX}/lib/inko/std

# The directory to place runtime library files in.
INSTALL_RT := ${INSTALL_PREFIX}/lib/inko/runtime/${RUNTIME_NAME}

# The directory the runtime library is located at at runtime.
RUNTIME_RT := ${PREFIX}/lib/inko/runtime

# The install path of the license file.
INSTALL_LICENSE := ${INSTALL_PREFIX}/share/licenses/inko/LICENSE

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
DOCS_FOLDER := main

# The directory to store temporary files in.
TMP_DIR := tmp

# The path of the archive to build for source releases.
SOURCE_TAR := ${TMP_DIR}/${VERSION}.tar.gz

# The path to the versions file.
MANIFEST := ${TMP_DIR}/manifest.txt

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
	clogs "${VERSION}"

release/versions:
	ruby scripts/update_versions.rb ${VERSION}

release/commit:
	git add */Cargo.toml Cargo.toml Cargo.lock CHANGELOG.md
	git commit -m "Release v${VERSION}"
	git push origin "$$(git rev-parse --abbrev-ref HEAD)"

release/tag:
	git tag -s -a -m "Release v${VERSION}" "v${VERSION}"
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
	cargo clean

docs/install:
	cd docs && poetry install --no-root

docs/build:
	cd docs && poetry run mkdocs build

docs/server:
	cd docs && poetry run mkdocs serve

docs/publish: docs/install docs/build
	aws s3 sync docs/build s3://${DOCS_S3_BUCKET}/manual/${DOCS_FOLDER} \
		--acl=public-read --delete --cache-control max-age=86400 --no-progress
	aws cloudfront create-invalidation \
		--distribution-id ${DOCS_CLOUDFRONT_ID} --paths "/*"

docs/versions:
	git tag | python ./scripts/docs_versions.py > versions.json
	aws s3 cp versions.json s3://${DOCS_S3_BUCKET}/manual/versions.json \
		--acl=public-read --cache-control max-age=86400
	aws cloudfront create-invalidation \
		--distribution-id ${DOCS_CLOUDFRONT_ID} --paths "/manual/versions.json"
	rm versions.json

runtimes:
	bash scripts/runtimes.sh ${VERSION}

.PHONY: release/source release/manifest release/changelog release/versions
.PHONY: release/commit release/publish release/tag
.PHONY: build install clean runtimes
.PHONY: docs/install docs/build docs/server docs/publish docs/versions
