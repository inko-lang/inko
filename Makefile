# The base directory to install the runtime in. Typically this will be either
# /usr or ~/.local.
PREFIX := /usr

# The version to use for building a source tarball.
RELEASE_VERSION != cat VERSION

# The architecture to use for building the VM.
ARCH != uname -m

# The directory to build Inko in.
BUILD := build

# The directory to install Inko into before bundling.
STAGING := ${BUILD}/staging

# The name of the tarball to build.
TARBALL := inko-${RELEASE_VERSION}-${ARCH}.tar.gz

# The path of the tarball to build.
TARBALL_PATH := ${BUILD}/${TARBALL}

# The name of the checksum file to generate.
CHECKSUM := ${TARBALL_PATH}.sha512

# The S3 bucket to upload source builds to.
BUCKET := releases.inko-lang.org

# The path of the manifest to manage.
MANIFEST := ${BUILD}/manifest.txt

# The Cloudfront distribution to use.
DISTRIBUTION := E3SFQ1OG1H5PCN

install:
	(cd compiler && make install)
	(cd runtime && make install PREFIX="${PREFIX}")
	(cd vm && make install PREFIX="${PREFIX}")

uninstall:
	(cd compiler && make uninstall)
	(cd runtime && make uninstall PREFIX="${PREFIX}")
	(cd vm && make uninstall PREFIX="${PREFIX}")

build-release:
	rm -rf "${STAGING}"
	mkdir -p "${STAGING}"
	(cd compiler && make build PREFIX="../${STAGING}")
	(cd runtime && make install PREFIX="../${STAGING}")
	(cd vm && make install PREFIX="../${STAGING}")
	cp VERSION "${STAGING}"
	cp LICENSE "${STAGING}"
	tar --directory "${STAGING}" --create --gzip --file "${TARBALL_PATH}" .
	sha512sum "${TARBALL_PATH}" | awk '{print $$1}' > "${CHECKSUM}"

upload-release: build-release
	aws s3 cp s3://${BUCKET}/manifest.txt "${MANIFEST}"
	echo "${TARBALL}" >> "${MANIFEST}"
	aws s3 cp --acl public-read "${TARBALL_PATH}" s3://${BUCKET}/
	aws s3 cp --acl public-read "${CHECKSUM}" s3://${BUCKET}/
	aws s3 cp --acl public-read "${MANIFEST}" s3://${BUCKET}/
	aws cloudfront create-invalidation \
		--distribution-id ${DISTRIBUTION} --paths "/*"

clean-release:
	rm -rf "${BUILD}"

.PHONY: install uninstall build-release upload-release clean-release
