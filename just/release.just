set no-cd

# Enabled to unrelated tasks don't trigger the evaluation of `VERSION` and thus
# a potential "cargo not found" command error.
set lazy

# The server to deploy to.
SERVER := "web.srv.yorickpeterse.com"

# The directory to deploy to.
TARGET := "/var/lib/shost/sites/releases.inko-lang.org"

# The directory to store temporary files in.
TMP_DIR := "tmp"

# The version to release.
VERSION := `cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2`

# The path to the source archive to create
ARCHIVE := f"{{ TMP_DIR }}/{{ VERSION }}.tar.gz"

# The name of the manifest file.
MANIFEST_NAME := "manifest.txt"

# The local path to the manifest.
MANIFEST := f"{{ TMP_DIR }}/{{ MANIFEST_NAME }}"

# The local path to the directory containing cross-compilation runtimes
RUNTIMES := "tmp/runtimes"

# The directory to upload runtime libraries to.
RUNTIMES_TARGET := f"/var/lib/shost/sites/releases.inko-lang.org/runtimes/{{ VERSION }}"

# Create the files and directories necessary for a release
prepare:
    mkdir -p "{{ TMP_DIR }}"

# Create the source archive
archive: prepare
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
    	| gzip > "{{ ARCHIVE }}"

# Build and upload a cross-compilation runtime.
runtime target:
    mkdir -p "{{ RUNTIMES }}/{{ target }}"
    cargo build -p rt --release
    cp target/release/libinko.a "{{ RUNTIMES }}/{{ target }}"
    tar --directory "{{ RUNTIMES }}" --create --gzip \
        --file "{{ RUNTIMES }}/{{ target }}.tar.gz" \
        "{{ target }}"
    scripts/scp.sh "{{ RUNTIMES }}/{{ target }}.tar.gz" \
        "{{ SERVER }}:{{ RUNTIMES_TARGET }}"

# Upload the source archive
source:
    scripts/scp.sh "{{ ARCHIVE }}" "{{ SERVER }}:{{ TARGET }}/{{ VERSION }}.tar.gz"

# Updates the release manifest
manifest:
    scripts/scp.sh "{{ SERVER }}:{{ TARGET }}/{{ MANIFEST_NAME }}" "{{ MANIFEST }}"
    echo "{{ VERSION }}" >> "{{ MANIFEST }}"
    sort --version-sort "{{ MANIFEST }}"
    scripts/scp.sh "{{ MANIFEST }}" "{{ SERVER }}:{{ TARGET }}/{{ MANIFEST_NAME }}"

# Update the changelog
changelog:
    clogs "{{ VERSION }}"

# Update the version in various places
versions:
    scripts/update_versions.sh "{{ VERSION }}"

# Create and push the release commit
commit:
    git add */Cargo.toml Cargo.toml Cargo.lock CHANGELOG.md
    git commit -m "Release v{{ VERSION }}"
    git push origin "$(git rev-parse --abbrev-ref HEAD)"

# Create and push the release tag
tag:
    git tag -a -m "Release v{{ VERSION }}" "v{{ VERSION }}"
    git push origin "v{{ VERSION }}"

# Publish the release
publish: versions changelog commit tag

# Remove release related files
clean:
    rm -rf "{{ TMP_DIR }}"
