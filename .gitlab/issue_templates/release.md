<!-- Replace NEW_VERSION with the version of the new release. -->

This issue tracks the work necessary to release version NEW_VERSION.

## General checklist

1. [ ] Run `make release VERSION=NEW_VERSION` on the `master` branch.
1. [ ] Make sure the pipeline for the tag passed.
1. [ ] Install the latest version using [ienv](https://gitlab.com/inko-lang/ienv)
   to make sure this works.
1. [ ] Create a merge request in <https://gitlab.com/inko-lang/website> (using
   the "news" template) to announce the release: LINK
1. [ ] Mention noteworthy changes based on the changelog, such as big features
   or breaking changes.

/label ~Release
