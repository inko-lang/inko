# The base directory to install the runtime in. Typically this will be either
# /usr or ~/.local.
PREFIX := /usr
ABS_PREFIX != ./scripts/realpath.sh "${PREFIX}"

# The architecture to use for building the VM.
ARCH != ./scripts/arch.sh

# The version to build.
VERSION != cat VERSION

# The name of the S3 bucket that contains all releases.
S3_BUCKET := releases.inko-lang.org

# The ID of the cloudfront distribution that serves all packages.
CLOUDFRONT_ID := E3SFQ1OG1H5PCN

# The directory to store temporary files in.
TMP_DIR := tmp

# The directory to use as a staging area for installing compiled files.
STAGING_DIR := ${TMP_DIR}/staging
ABS_STAGING_DIR != ./scripts/realpath.sh "${STAGING_DIR}"

# The path of the archive to build for source releases.
SOURCE_TAR := ${TMP_DIR}/inko-${VERSION}-source.tar.gz

# The path of the checksum for the source tar archive.
SOURCE_TAR_CHECKSUM := ${SOURCE_TAR}.sha256

# The path of the archive to build for precompiled releases.
COMPILED_TAR := ${TMP_DIR}/inko-${VERSION}-compiled-${ARCH}.tar.gz

# The path of the checksum for the compiled tar archive.
COMPILED_TAR_CHECKSUM := ${COMPILED_TAR}.sha256

# The path to the manifest file.
MANIFEST := ${TMP_DIR}/manifest.txt

${TMP_DIR}:
	mkdir -p "${TMP_DIR}"

${STAGING_DIR}:
	mkdir -p "${STAGING_DIR}"

${SOURCE_TAR}: ${TMP_DIR} ${REPO_DIR}
	git archive --format tar HEAD \
		compiler/bin \
		compiler/lib \
		compiler/Makefile \
		compiler/README.md \
		runtime/src \
		runtime/Makefile \
		runtime/README.md \
		vm/src \
		vm/Cargo.toml \
		vm/Cargo.lock \
		vm/Makefile \
		vm/README.md \
		LICENSE \
		Makefile \
		README.md \
		VERSION \
		scripts \
		| gzip > "${SOURCE_TAR}"

${SOURCE_TAR_CHECKSUM}: ${SOURCE_TAR}
	sha256sum "${SOURCE_TAR}" | awk '{print $$1}' > "${SOURCE_TAR_CHECKSUM}"

${COMPILED_TAR}: ${TMP_DIR} ${STAGING_DIR} ${REPO_DIR}
	$(MAKE) install PREFIX="${ABS_STAGING_DIR}"
	cp LICENSE "${STAGING_DIR}/LICENSE"
	tar --directory "${STAGING_DIR}" --create --gzip --file "${COMPILED_TAR}" .

${COMPILED_TAR_CHECKSUM}: ${COMPILED_TAR}
	sha256sum "${COMPILED_TAR}" | awk '{print $$1}' > "${COMPILED_TAR_CHECKSUM}"

clean:
	rm -rf "${TMP_DIR}"

# Builds a tar archive containing just the source code.
release-source: ${SOURCE_TAR} ${SOURCE_TAR_CHECKSUM}
	aws s3 cp --acl public-read "${SOURCE_TAR}" s3://${S3_BUCKET}/inko/
	aws s3 cp --acl public-read "${SOURCE_TAR_CHECKSUM}" s3://${S3_BUCKET}/inko/

# Builds a tar archive containing various precompiled components (e.g. the VM).
release-compiled: ${COMPILED_TAR} ${COMPILED_TAR_CHECKSUM}
	aws s3 cp --acl public-read "${COMPILED_TAR}" s3://${S3_BUCKET}/inko/
	aws s3 cp --acl public-read "${COMPILED_TAR_CHECKSUM}" s3://${S3_BUCKET}/inko/

# Rebuilds the manifest from scratch.
rebuild-manifest: ${TMP_DIR}
	aws s3 ls s3://${S3_BUCKET}/inko/ | \
		grep -oP '(inko-.+tar\.gz$$)' | \
		sort > "${MANIFEST}"
	aws s3 cp --acl public-read "${MANIFEST}" s3://${S3_BUCKET}/inko/
	aws cloudfront create-invalidation --distribution-id ${CLOUDFRONT_ID} \
		--paths "/inko/*"

# Installs all components into a prefix directory.
install:
	(cd compiler && $(MAKE) install PREFIX="${ABS_PREFIX}")
	(cd runtime && $(MAKE) install PREFIX="${ABS_PREFIX}")
	(cd vm && $(MAKE) install PREFIX="${ABS_PREFIX}")

# Removes all components from a prefix directory.
uninstall:
	(cd compiler && $(MAKE) uninstall PREFIX="${ABS_PREFIX}")
	(cd runtime && $(MAKE) uninstall PREFIX="${ABS_PREFIX}")
	(cd vm && $(MAKE) uninstall PREFIX="${ABS_PREFIX}")

# Tags the current version in Git.
tag:
	git tag -a -m "Release v${VERSION}" "v${VERSION}"
	git push origin "v${VERSION}"

.PHONY: clean release-source release-compiled install uninstall rebuild-manifest tag
