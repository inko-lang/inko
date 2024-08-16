---
name: Release
title: Release X.Y.Z
about: Tasks for a new release of Inko
labels: release
---

Prepare the release:

- [ ] Run `make release/publish VERSION=X` in `main`
- [ ] Make sure the tag pipeline passes
- [ ] Run the [AUR](https://github.com/inko-lang/aur/actions/workflows/release.yml) pipeline
- [ ] Run the [Copr](https://github.com/inko-lang/copr/actions/workflows/release.yml) pipeline

Publish the release:

- [ ] Set up a pull
- [ ] Update `source/documentation.md` to include links to the manual and standard library of the specific release
- [ ] Merge the release post

Announce the release:

- [ ] Announce in /r/inko
- [ ] Announce in /r/ProgrammingLanguages
- [ ] Announce in `#announcements` in Discord
- [ ] Announce in `#inko` in the /r/ProgrammingLanguages Discord
- [ ] Create a [GitHub announcement](https://github.com/orgs/inko-lang/discussions/new?category=announcements)
