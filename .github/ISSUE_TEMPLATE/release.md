---
name: Release
title: Release X.Y.Z
about: Tasks for a new release of Inko
labels: release
---

- [ ] Set up a pull request with the release post and add it here: LINK
- [ ] Run `make release/publish VERSION=X` in `main`, where `X` is the new version
- [ ] Make sure the tag pipeline passes
- [ ] Run the [AUR release pipeline](https://github.com/inko-lang/aur/actions/workflows/release.yml), using `inko` as the package name
- [ ] Update the [copr](https://github.com/yorickpeterse/copr/blob/main/inko) package
- [ ] Merge the release post
- [ ] Announce in /r/inko
- [ ] Announce in /r/ProgrammingLanguages
- [ ] Announce in `#announcements` in Discord
- [ ] Announce in `#inko` in the /r/ProgrammingLanguages Discord
