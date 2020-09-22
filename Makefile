# The directory to move files into as part of the installation procedure.
DESTDIR :=

# The base directory for executables, loading compiler source files, etc.
# This path _must_ start with a /.
PREFIX := /usr

ifneq (${DESTDIR},)
	INSTALL_PREFIX = ${DESTDIR}${PREFIX}
else
	INSTALL_DIR = ${PREFIX}
endif

# The directory to place executable files in.
BIN_DIR := ${INSTALL_PREFIX}/bin

# The base directory to place all library files (e.g. the runtime source) in.
LIB_DIR := ${INSTALL_PREFIX}/lib/inko

# The directory to place the compiler source code in.
COMPILER_LIB_DIR := ${LIB_DIR}/compiler

# The directory to load compiler source files from at runtime.
COMPILER_RUNTIME_LIB_DIR := ${PREFIX}/lib/inko/compiler

# The directory to place the runtime source code in.
RUNTIME_LIB_DIR := ${LIB_DIR}/runtime

# The directory to place the license in.
LICENSE_DIR := ${INSTALL_PREFIX}/share/licenses/inko

# The cargo command to use for building the VM.
CARGO_CMD := cargo

# The target to use for cross compilation. An empty string indicates the default
# target of the underlying platform.
TARGET :=

# The list of features to enable when building the VM.
FEATURES :=

# Additional flags to pass to rustc.
RUSTFLAGS :=

# The architecture to use for building the VM.
ARCH != scripts/arch.sh

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

# Add any default RUSTFLAGS variables, unless we a custom RUSTFLAGS is
# specified.
ifeq (${RUSTFLAGS},)
# On x86-64 we want to enable AES-NI support.
ifneq (,$(findstring x86_64,$(ARCH)))
	RUSTFLAGS += -C target-feature=+aes
endif
endif

# The version to build.
VERSION != cat VERSION

# The name of the S3 bucket that contains all releases.
S3_BUCKET := releases.inko-lang.org

# The ID of the cloudfront distribution that serves all packages.
CLOUDFRONT_ID := E3SFQ1OG1H5PCN

# The directory to store temporary files in.
TMP_DIR := tmp

# The path of the archive to build for source releases.
SOURCE_TAR := ${TMP_DIR}/inko-${VERSION}-source.tar.gz

# The path of the checksum for the source tar archive.
SOURCE_TAR_CHECKSUM := ${SOURCE_TAR}.sha256

# The path to the manifest file.
MANIFEST := ${TMP_DIR}/manifest.txt

# The program to use for generating SHA256 checksums.
SHA256SUM := sha256sum

# If DEV is set to a non-empty values, the CLI will use the local copy of the
# compiler and runtime.
#
# We export these here so we don't have to explicitly pass them to every command
# used to build the CLI.
ifneq (${DEV},)
	export INKO_COMPILER_BIN = $(realpath compiler/bin/inkoc)
	export INKO_COMPILER_LIB = $(realpath compiler/lib)
	export INKO_RUNTIME_LIB = $(realpath runtime/src)
endif

${TMP_DIR}:
	mkdir -p "${@}"

${SOURCE_TAR}: ${TMP_DIR}
	git archive --format tar HEAD \
		compiler/bin \
		compiler/lib \
		compiler/install.rb \
		runtime/src \
		vm/src \
		vm/Cargo.toml \
		vm/Cargo.lock \
		LICENSE \
		Makefile \
		VERSION \
		scripts \
		| gzip > "${@}"

${SOURCE_TAR_CHECKSUM}: ${SOURCE_TAR}
	${SHA256SUM} "${SOURCE_TAR}" | awk '{print $$1}' > "${SOURCE_TAR_CHECKSUM}"

release/source: ${SOURCE_TAR} ${SOURCE_TAR_CHECKSUM}
	aws s3 cp --acl public-read "${SOURCE_TAR}" s3://${S3_BUCKET}/inko/
	aws s3 cp --acl public-read "${SOURCE_TAR_CHECKSUM}" s3://${S3_BUCKET}/inko/

release/manifest: ${TMP_DIR}
	aws s3 ls s3://${S3_BUCKET}/inko/ | \
		grep -oP '(inko-.+tar\.gz$$)' | \
		sort > "${MANIFEST}"
	aws s3 cp --acl public-read "${MANIFEST}" s3://${S3_BUCKET}/inko/
	aws cloudfront create-invalidation --distribution-id ${CLOUDFRONT_ID} \
		--paths "/inko/*"

release/changelog:
	ruby scripts/changelog.rb

release/versions:
	ruby scripts/update_versions.rb ${VERSION}

release/commit:
	git commit VERSION compiler/lib/inkoc/version.rb vm/Cargo.toml \
		vm/Cargo.lock CHANGELOG.md -m "Release v${VERSION}"
	git push origin "$(git rev-parse --abbrev-ref HEAD)"

release/tag:
	git tag -a -m "Release v${VERSION}" "v${VERSION}"
	git push origin "v${VERSION}"

release/publish: release/versions release/changelog release/commit release/tag

${BIN_DIR}/inkoc:
	install -D -m755 compiler/bin/inkoc "${@}"
	ruby compiler/install.rb "${COMPILER_RUNTIME_LIB_DIR}" "${@}"

${COMPILER_LIB_DIR}:
	mkdir -p "${@}"
	cp -r compiler/lib/* "${@}"

${RUNTIME_LIB_DIR}:
	mkdir -p "${@}"
	cp -r runtime/src/* "${@}"

${BIN_DIR}/inko:
	install -D -m755 ${TARGET_BINARY} "${@}"

${LICENSE_DIR}/LICENSE:
	install -D -m 644 LICENSE "${@}"

# Building is a separate step so that environment variables such as DESTDIR are
# not passed to any crates we need to build, ensuring they don't break because
# of that (example: https://github.com/tov/libffi-sys-rs/issues/35).
build: vm/release

install: ${BIN_DIR}/inko ${BIN_DIR}/inko-test ${BIN_DIR}/inkoc \
	${COMPILER_LIB_DIR} ${RUNTIME_LIB_DIR} ${BIN_DIR}/inko ${LICENSE_DIR}/LICENSE

uninstall:
	rm -f "${BIN_DIR}/inko"
	rm -f "${BIN_DIR}/inkoc"
	rm -f "${BIN_DIR}/inko"
	rm -f "${BIN_DIR}/inko-test"
	rm -rf "${LICENSE_DIR}"
	rm -rf "${LIB_DIR}"

clean:
	rm -rf "${TMP_DIR}"
	cd vm && ${CARGO_CMD} clean

compiler/test:
	cd compiler && bundle exec rspec spec

runtime/test:
	$(MAKE) vm/release DEV=1
	./${TARGET_BINARY} test -d runtime/tests dummy

vm/debug:
	cd cli && ${CARGO_CMD} build ${TARGET_OPTION} ${FEATURES_OPTION}

vm/check:
	cd cli && ${CARGO_CMD} check ${TARGET_OPTION} ${FEATURES_OPTION}

vm/test:
	cd vm && env CARGO_INCREMENTAL=1 ${CARGO_CMD} test ${FEATURES_OPTION}

vm/clippy:
	touch vm/src/lib.rs cli/src/lib.rs
	cd cli && ${CARGO_CMD} clippy ${TARGET_OPTION} ${FEATURES_OPTION} -- -Dwarnings

vm/rustfmt-check:
	rustfmt --check vm/src/lib.rs cli/src/main.rs

vm/rustfmt:
	rustfmt --emit files vm/src/lib.rs cli/src/main.rs

vm/release:
	cd cli && env RUSTFLAGS="${RUSTFLAGS}" \
		${CARGO_CMD} build --release ${TARGET_OPTION} ${FEATURES_OPTION}

vm/profile:
	cd cli && env RUSTFLAGS="-g ${RUSTFLAGS}" \
		${CARGO_CMD} build --release ${TARGET_OPTION} ${FEATURES_OPTION}

.PHONY: release/source release/manifest release/changelog release/versions
.PHONY: release/commit release/publish release/tag
.PHONY: build install uninstall clean
.PHONY: vm/debug vm/check vm/clippy vm/rustfmt-check vm/rustfmt vm/release vm/profile
.PHONY: test/vm test/runtime test/compiler
.DEFAULT: build
