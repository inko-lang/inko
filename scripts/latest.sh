#!/usr/bin/env bash

set -e

USER=web
KEY="~/.ssh/id_ed25519"
HOSTS="scripts/known_hosts"
BASE="${1}"
REF="${2}"

if [[ "${REF}" != $(git tag -l --sort=-v:refname | head -n1) ]]
then
    echo 'Nothing to update'
    exit 0
fi

if [[ -v SSH_PRIVATE_KEY ]]
then
    echo -e "${SSH_PRIVATE_KEY}" > deploy_key
    chmod 600 deploy_key
    KEY="deploy_key"
fi

ssh -o "User=${USER}" \
    -o "UserKnownHostsFile=${HOSTS}" \
    -i "${KEY}" \
    "rm ${BASE}/latest && ln -s ${BASE}/${REF} ${BASE}/latest"

if [[ -v SSH_PRIVATE_KEY ]]
then
    rm deploy_key
fi
