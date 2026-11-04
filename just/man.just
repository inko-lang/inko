set working-directory := "../docs"

# The base directory/site to deploy to.
TARGET := "/var/lib/shost/sites/docs.inko-lang.org"

# Build the manual
build ref="main":
    rm -rf public
    inko build --release
    env DOCS_REF="{{ ref }}" ./build/release/main

# Watch and automatically rebuild the manual
watch ref="main":
    env DOCS_REF="{{ ref }}" scripts/watch.sh

# Publish the manual
publish ref="main": build
    ../scripts/docs.sh public "{{ TARGET }}/manual" "{{ ref }}"
