# The version of the _compiler_, not the runtime to target.
VERSION != cat VERSION

test:
	bundle exec rspec spec

pkg/inkoc-${VERSION}.gem:
	mkdir -p pkg
	gem build --silent inkoc.gemspec
	mv inkoc-${VERSION}.gem pkg/

build: pkg/inkoc-${VERSION}.gem

install: build
	gem install "pkg/inkoc-${VERSION}.gem"

uninstall:
	gem uninstall -x inkoc

.PHONY: test install uninstall
