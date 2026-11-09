set no-cd

# The base directory/site to deploy to.
TARGET := "/var/lib/shost/sites/docs.inko-lang.org"

# Build or publish the documentation
ci ref="main":
    just std {{ if ref == "main" { "publish" } else { "build" } }} {{ ref }}
    just std latest {{ ref }}

# Build the documentation
build:
    rm -rf std/build
    cargo build
    cd std && idoc --compiler ../target/debug/inko

# Publish the documentation
publish ref="main": build
    scripts/docs.sh std/build/idoc/public "{{ TARGET }}/std" "{{ ref }}"

# Update the "latest" tag.
latest ref:
    scripts/latest.sh "{{ TARGET }}/std" "{{ ref }}"
