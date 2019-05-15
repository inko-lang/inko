<!-- Replace NEW_VERSION with the version of the new release. -->

This issue tracks the work necessary to release version NEW_VERSION.

## General checklist

1. [ ] Run `make release VERSION=NEW_VERSION` on the `master` branch.
1. [ ] Make sure the pipeline for the tag passed.
1. [ ] Install the latest version using [ienv](https://gitlab.com/inko-lang/ienv)
   to make sure this works.
1. [ ] Create a merge request in <https://gitlab.com/inko-lang/website> to
   announce the release: LINK
1. [ ] Mention noteworthy changes based on the changelog, such as big features
   or breaking changes.

## Before publishing

1. [ ] Check for common spelling errors (`setlocal spell spelllang=en` in Vim)
1. [ ] Make sure all links (if any) are working
1. [ ] Preview locally to make sure the article is rendering properly

## After publishing

1. [ ] Submit to <https://www.reddit.com/r/inko/>
1. [ ] Submit to <https://discourse.inko-lang.org/c/announcements>
1. [ ] Tweet about it
1. [ ] Post a link to the article in the Matrix channel

/label ~Release
