# The version of the _compiler_, not the runtime to target.
VERSION != cat VERSION

# The location to store the built Gem in.
PREFIX := pkg

test:
	bundle exec rspec spec

${PREFIX}/inkoc-${VERSION}.gem:
	mkdir -p ${PREFIX}
	gem build inkoc.gemspec
	mv inkoc-${VERSION}.gem ${PREFIX}/

build: ${PREFIX}/inkoc-${VERSION}.gem

install: build
	(cd "${PREFIX}" && gem install "$${PWD}/inkoc-${VERSION}.gem")

uninstall:
	gem uninstall -x inkoc

.PHONY: test install uninstall
