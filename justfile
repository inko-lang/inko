# Publishing of releases
mod release 'just/release.just'

# Generating and deploying of the manual
mod man 'just/man.just'

# Generating and deploying of the stdlib documentation
mod std 'just/std.just'

# Building of CI OCI images
mod ci 'just/ci.just'

# Building of the OCI image
mod image 'just/image.just'

# The server to deploy to.
SERVER := "web.srv.yorickpeterse.com"

# Update the list of known server hosts.
hosts:
    ssh-keyscan -q "{{ SERVER }}" > scripts/known_hosts
