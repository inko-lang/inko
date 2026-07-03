#!/usr/bin/env bash

set -e

USER=root
PORT=2222
KEY="~/.ssh/id_ed25519"
HOSTS="scripts/known_hosts"

if [[ -v SSH_PRIVATE_KEY ]]
then
    echo -e "${SSH_PRIVATE_KEY}" > deploy_key
    chmod 600 deploy_key
    KEY="deploy_key"
fi

scp -o "User=${USER}" \
    -o "UserKnownHostsFile=${HOSTS}" \
    -i "${KEY}" \
    -P "${PORT}" \
    $@

if [[ -v SSH_PRIVATE_KEY ]]
then
    rm deploy_key
fi
