#!/usr/bin/env bash

set -e

SERVER=web.srv.yorickpeterse.com
VERSION="$(cargo pkgid -p inko | cut -d\# -f2 | cut -d: -f2)"
DIR="tmp/runtimes"

scripts/scp.sh -r "${DIR}" \
    "${SERVER}:/var/lib/shost/releases.inko-lang.org/runtimes/${VERSION}"
