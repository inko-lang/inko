#!/usr/bin/env bash

set -e

USER=root
SERVER=web.hetzner.yorickpeterse.com
PORT=2222
SRC="${1}"
DEST="${2}"
REF="${3}"
DIR="docs.${REF}"
TAR="${DIR}.tar.gz"

echo 'Creating archive...'
tar --directory "${SRC}" --create --gzip --file "${TAR}" .

echo 'Uploading archive...'
scp -P "${PORT}" "${TAR}" "${USER}@${SERVER}:${DEST}"

echo 'Extracting archive...'
ssh -p "${PORT}" "${USER}@${SERVER}" "cd ${DEST} && mkdir ${DIR} && tar --extract --directory ${DIR} --file ${TAR} && rm ${TAR}"

echo 'Deploying changes...'
ssh -p "${PORT}" "${USER}@${SERVER}" "cd ${DEST} && if [[ -d "${REF}" ]]; then mv ${REF} ${REF}.old; fi && mv ${DIR} ${REF} && rm -rf ${REF}.old"

echo 'Cleaning up...'
rm "${TAR}"
