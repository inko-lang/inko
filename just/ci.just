set unstable
set working-directory := '../ci'

# The host of the registry.
HOST := "ghcr.io"

# The user to use for logging in to the registry.
USER := "yorickpeterse"

# The namespace of the repository.
NAMESPACE := "inko-lang"

# The name of the repository.
REPO := "ci"

# Build an image
build image tag="latest":
    buildah build \
        --pull=newer \
        --isolation=chroot \
        --layers \
        --tag "{{ fqn(image, tag) }}" \
        {{ image }}

# Log in to the container registry
login pass:
    @buildah login {{ HOST }} --username "{{ USER }}" --password "{{ pass }}"

# Push the container to the registry
push name tag:
    buildah push "{{ fqn(name, tag) }}"

# Create and push a manifest of multi-arch tags
manifest name tag:
    buildah manifest create \
        --all "{{ fqn(name, 'manifest') }}" \
        "{{ fqn(name, f'{{ tag }}-amd64') }}" \
        "{{ fqn(name, f'{{ tag }}-arm64') }}"
    buildah manifest push "{{ fqn(name, 'manifest') }}" "docker://{{ fqn(name, tag) }}"
    buildah manifest rm "{{ fqn(name, 'manifest') }}"

# Prune all images
clean:
    buildah prune --all --force

fqn(name, tag) := f"{{ HOST }}/{{ NAMESPACE }}/{{ REPO }}/{{ name }}:{{ tag }}"
