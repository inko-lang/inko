#!/usr/bin/env bash
set -e

function audit() {
    local report="gl-dependency-scanning-report.json"

    cargo audit -f vm/Cargo.lock --json > $report

    if [[ ! -s $report ]]
    then
        echo '[]' > $report
    fi
}

audit
