#!/usr/bin/env bash

set -e

VERSION="$(cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2)"
DIR="tmp/runtimes"

scripts/rclone.sh sync "${DIR}" \
    ":sftp:/var/lib/shost/releases.inko-lang.org/runtimes/${VERSION}"
