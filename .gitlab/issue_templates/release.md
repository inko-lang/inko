<!-- Replace NEW_VERSION with the version of the new release. -->

This issue tracks the work necessary to release version NEW_VERSION.

## Checklist

1. [ ] Run `make release VERSION=NEW_VERSION` on the `master` branch.
1. [ ] Make sure the pipeline for the tag passed.
1. [ ] Install the latest version using [ienv](https://gitlab.com/inko-lang/ienv)
   to make sure this works.
1. [ ] Create a merge request in <https://gitlab.com/inko-lang/website> to
   announce the release, using the "news" template.
1. [ ] Mention noteworthy changes based on the changelog, such as big features
   or breaking changes.
1. [ ] Followed the "news" checklist in the website repository.

/label ~Release
