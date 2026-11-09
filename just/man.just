set working-directory := "../docs"

# The base directory/site to deploy to.
TARGET := "/var/lib/shost/sites/docs.inko-lang.org"

# Build or publish the manual
ci ref="main":
    just man {{ if ref == "main" { "publish" } else { "build" } }} {{ ref }}
    just man latest {{ ref }}

# Build the manual
build ref="main":
    rm -rf public
    inko build --release
    env DOCS_REF="{{ ref }}" ./build/release/main

# Watch and automatically rebuild the manual
watch ref="main":
    env DOCS_REF="{{ ref }}" scripts/watch.sh

# Publish the manual
publish ref="main": (build ref)
    ../scripts/docs.sh public "{{ TARGET }}/manual" "{{ ref }}"

# Update the "latest" tag.
latest ref:
    ../scripts/latest.sh "{{ TARGET }}/manual" "{{ ref }}"
