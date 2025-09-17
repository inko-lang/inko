# Code generation tools

This directory contains various tools for generating standard library modules.
Most notably it's used for generating perfect hash functions.

The various tasks are run using `make` in this `gen/` directory.

## Perfect hashing

For perfect hashing we use a vendored copy of the Python code from
<https://github.com/ilanschnell/perfect-hash/>, with a custom template to
produce Inko source code.

The hash functions return an `Int` instead of `Option[Int]` as benchmarking
revealed the former to be 2-3x faster.

## Requirements

- Make
- Python 3
- [jq](https://jqlang.org/)

## MIME data

The modules `src/std/mime/phf.inko` and `src/std/mime/data.inko` are
generated/updated using `make mime`. The source data is located at
`data/mime.json` and is a JSON file in the following format:

```json
{
  "file extension": ["primary mime type", "extra mime type", ...]
}
```

The keys in this JSON object _must_ be sorted in alphabetical order (A-Z).

To add a new MIME type or file extension:

1. Add the entry to the JSON file, make sure the sort order of the file
   extensions is still correct (i.e. don't just dump a new file extension at the
   end of the file)
1. Run `make mime` to update the source files. This can take 10-20 seconds
1. Run `inko test` in the parent directory to verify all tests still pass

Note that we only accept additions for officially recognized file types (i.e.
those in the [IANA media types
registry](https://www.iana.org/assignments/media-types/media-types.xhtml)).
