#!/usr/bin/env bash

set -e

VERSION="$(cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2)"
DIR="tmp/runtimes"

rclone sync --config rclone.conf --checksum --verbose "${DIR}" \
    "production:inko-releases/runtimes/${VERSION}"
