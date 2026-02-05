#!/usr/bin/env bash

set -e

USER=root
SERVER=web
PORT=2222

run() {
    rclone "${1}" --verbose \
        --multi-thread-streams 8 \
        --transfers 8 \
        --metadata \
        --checksum \
        --no-update-dir-modtime \
        --sftp-host "$(hcloud server ip "${SERVER}")" \
        --sftp-user "${USER}" \
        --sftp-port "${PORT}" \
        ${@:4} \
        "${2}" "${3}"
}

if [[ -v SSH_PRIVATE_KEY ]]
then
    echo -e "${SSH_PRIVATE_KEY}" > deploy_key
    run "${1}" "${2}" "${3}" --sftp-key-file deploy_key
    rm deploy_key
else
    run "${1}" "${2}" "${3}"
fi
