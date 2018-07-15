VERSION != cat VERSION

test:
	bundle exec rspec spec

install:
	gem build --silent inkoc.gemspec
	gem install "inkoc-${VERSION}.gem"

uninstall:
	gem uninstall inkoc

.PHONY: test install uninstall
