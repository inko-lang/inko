set no-cd

# Update the changelog
changelog version:
    clogs "{{ version }}"

# Create and push the release commit
commit version:
    git add .
    git commit -m "Release v{{ version }}"
    git push origin "$(git rev-parse --abbrev-ref HEAD)"

# Create the release tag
tag version:
    git tag -a -m "Release v{{ version }}" "v{{ version }}"
    git push origin "v{{ version }}"

# Update the version in the README
update-readme version:
    sed -E -i -e 's/^inko pkg add ([^ ]+).+$/inko pkg add \1 {{ version }}/' README.md

# Publish a new release
publish version: (update-readme version) (changelog version) (commit version) (tag version)
