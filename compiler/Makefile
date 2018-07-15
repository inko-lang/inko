# The version of the _compiler_, not the runtime to target.
VERSION != cat VERSION

test:
	bundle exec rspec spec

install:
	gem build --silent inkoc.gemspec
	gem install "inkoc-${VERSION}.gem"
	rm "inkoc-${VERSION}.gem"

uninstall:
	gem uninstall -x inkoc

.PHONY: test install uninstall
