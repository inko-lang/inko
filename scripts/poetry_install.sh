#!/usr/bin/env sh

# `poetry install` doesn't handle all connection errors, such as connection
# reset errors. This script is used in CI to try and install all dependencies up
# to 3 times. This script is separate from our Makefile, as it's not really
# needed outside of CI environments.

install() {
    retries=0

    while [ "$retries" -le 2 ]
    do
        if (cd docs && poetry install)
        then
            return
        fi

        echo 'poetry install failed, retrying...'

        retries=$((retries + 1))
    done

    echo 'Failed to install poetry dependencies'
    exit 1
}

install
