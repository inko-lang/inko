#!/usr/bin/env bash

set -e

VERSION="$(cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2)"

rclone sync --config rclone.conf --checksum --verbose tmp/runtimes \
    "production:inko-releases/runtimes/$VERSION"
