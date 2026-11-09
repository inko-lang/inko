set unstable
set no-cd

# The host of the registry.
HOST := "ghcr.io"

# The user to use for logging in to the registry.
USER := "yorickpeterse"

# The namespace of the repository.
NAMESPACE := "inko-lang"

# The name of the repository.
REPO := "inko"

# Build an image
build tag="latest":
    buildah build --pull=newer --isolation=chroot --layers --tag "{{ fqn(tag) }}"

# Log in to the container registry
login pass:
    @buildah login {{ HOST }} --username "{{ USER }}" --password "{{ pass }}"

# Push the container to the registry
push tag:
    buildah push "{{ fqn(tag) }}"

# Create and push a manifest of multi-arch tags
manifest tag:
    buildah manifest create \
        --all "{{ fqn('manifest') }}" \
        "{{ fqn(f'{{ tag }}-amd64') }}" \
        "{{ fqn(f'{{ tag }}-arm64') }}"
    buildah manifest push "{{ fqn('manifest') }}" "docker://{{ fqn(tag) }}"
    buildah manifest rm "{{ fqn('manifest') }}"

# Prune all images
clean:
    buildah prune --all --force

fqn(tag) := f"{{ HOST }}/{{ NAMESPACE }}/{{ REPO }}/inko:{{ tag }}"
