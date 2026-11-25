---
name: Release
title: Release X.Y.Z
about: Tasks for a new release of Inko
labels:
  - type/release
---

Prepare the release:

- [ ] Run `just release::VERSION=X.Y.Z release publish` in `main`
- [ ] Make sure the tag pipeline passes
- [ ] Run `ivm install X` to make sure the release artifacts are present
- [ ] Run the [AUR](https://github.com/inko-lang/aur/actions/workflows/release.yml) pipeline
- [ ] Run the [Copr](https://github.com/inko-lang/copr/actions/workflows/release.yml) pipeline

Publish the release:

- [ ] Set up a pull request for the release post in https://github.com/inko-lang/website
- [ ] Update `source/documentation.md` to include links to the manual and standard library of the specific release
- [ ] Merge the release post

Announce the release:

- [ ] Announce in /r/inko
- [ ] Announce in /r/ProgrammingLanguages
- [ ] Announce in `#announcements` in Discord
- [ ] Announce in `#inko` in the /r/ProgrammingLanguages Discord
- [ ] Create a [GitHub announcement](https://github.com/orgs/inko-lang/discussions/new?category=announcements)
