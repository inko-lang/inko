# Publishing of releases
mod release 'just/release.just'

# Generating and deploying of the manual
mod man 'just/man.just'

# Generating and deploying of the stdlib documentation
mod std 'just/std.just'

# The server to deploy to.
SERVER := "web.srv.yorickpeterse.com"

# Update the list of known server hosts.
hosts:
    ssh-keyscan -q "{{ SERVER }}" > scripts/known_hosts
