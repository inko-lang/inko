#!/usr/bin/env bash

set -e

USER=root
SERVER=web.srv.yorickpeterse.com
PORT=2222
SRC="${1}"
DEST="${2}"
REF="${3}"
DIR="docs.${REF}"
TAR="${DIR}.tar.gz"
KEY="~/.ssh/id_ed25519"
HOSTS="scripts/known_hosts"

if [[ -v SSH_PRIVATE_KEY ]]
then
    echo -e "${SSH_PRIVATE_KEY}" > deploy_key
    chmod 600 deploy_key
    KEY="deploy_key"
fi

echo 'Creating archive...'
tar --directory "${SRC}" --create --gzip --file "${TAR}" .

echo 'Uploading archive...'
scp -o "UserKnownHostsFile=${HOSTS}" \
    -i "${KEY}" -P "${PORT}" "${TAR}" "${USER}@${SERVER}:${DEST}"

echo 'Extracting archive...'
ssh -o "UserKnownHostsFile=${HOSTS}" \
    -i "${KEY}" \
    -p "${PORT}" \
    "${USER}@${SERVER}" \
    "cd ${DEST} && mkdir ${DIR} && tar --extract --directory ${DIR} --file ${TAR} && rm ${TAR}"

echo 'Deploying changes...'
ssh -o "UserKnownHostsFile=${HOSTS}" \
    -i "${KEY}" \
    -p "${PORT}" \
    "${USER}@${SERVER}" \
    "cd ${DEST} && if [[ -d "${REF}" ]]; then mv ${REF} ${REF}.old; fi && mv ${DIR} ${REF} && rm -rf ${REF}.old"

echo 'Cleaning up...'
rm "${TAR}"

if [[ -v SSH_PRIVATE_KEY ]]
then
    rm deploy_key
fi
