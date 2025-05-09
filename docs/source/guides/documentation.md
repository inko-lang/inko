---
{
  "title": "Generating documentation"
}
---

Documentation is written using
[Markdown](https://en.wikipedia.org/wiki/Markdown), specifically using the
[inko-markdown](https://github.com/yorickpeterse/inko-markdown) dialect.

Symbols (methods, types, etc) are documented by placing one or more comments
before them, without empty lines between the comments or between the last
comment and the start of the symbol:

```inko
# This is the documentation for the constant.
# It happens to occupy two lines.
let NUMBER = 42
```

Modules are documented by placing comments at the start of the module:

```inko
# This is the documentation for the module.
import std.string (StringBuffer)

fn example {}
```

If the module documentation is followed by a symbol (e.g. a type), ensure
there's an empty line after the comment, otherwise it's treated as the
documentation for the symbol:

```inko
# This documents the _module_ and not the type.

type Example {}
```

The following can be documented:

- Modules
- Constants
- Module methods
- Classes
- Traits
- Methods defined on a type
- Methods defined in an `impl` block
- Methods defined in a trait

## Generating documentation

Documentation is generated by processing a collection of JSON files produced by
the `inko doc` command, and turning these into something useful such as a static
website.

[idoc](https://github.com/inko-lang/idoc) is a tool that converts these JSON
files to a self-contained static website. idoc is written in Inko and is
installed separately. Using idoc you don't need to run `inko doc` yourself.

### Installation

To install from source:

```bash
git clone https://github.com/inko-lang/idoc.git
cd idoc
make install PREFIX=~/.local
```

This builds idoc and installs it into `~/.local`, such that the executable is
found at `~/.local/bin/idoc`.

::: tip
Make sure `~/.local/bin` is in your `PATH`, otherwise you need to use the full
path to the `idoc` executable when generating documentation.
:::

You can also use idoc's official Docker/Podman image:

```bash
# Using Docker:
docker pull ghcr.io/inko-lang/idoc:latest

# Using Podman:
podman pull ghcr.io/inko-lang/idoc:latest
```

### Usage

Using idoc, you don't need to run `inko doc` yourself, instead you just run
`idoc` in your project directory and the resulting static website is found at
`./build/idoc/public`:

```bash
$ idoc
$ ls build/idoc/public/
css  favicon.ico  index.html  js  module  search.json
```

To also generate documentation for the dependencies of your project (including
the standard library), use the `--dependencies` option:

```bash
idoc --dependencies
```

To include documentation of private symbols (excluded by default), use the
`--private` option:

```bash
idoc --private
```

For more information, refer to the output of `idoc --help`.

### Using Docker/Podman

If you don't want to install idoc from source, you can also use Docker or
Podman:

```bash
# Using Docker:
docker run --rm --volume $PWD:$PWD:z --workdir $PWD idoc:latest idoc

# Using Podman:
podman run --rm --volume $PWD:$PWD:z --workdir $PWD idoc:latest idoc
```

The `--volume` option is needed so the container has access to your project's
source code, while the `--workdir` option ensures `idoc` runs inside your
project's working directory.

## Deploy to GitHub Pages

Users of GitHub and [GitHub Actions](https://docs.github.com/en/actions) can
deploy their documentation using [GitHub Pages](https://pages.github.com/). This
is done as follows:

1. Navigate to your GitHub project, then go to "Settings" and click "Pages" in
   the left sidebar
1. For the source, choose "GitHub Actions"
1. In the left sidebar, click "Environments" then click on the "github-pages"
   environment name
1. Under the section "Deployment branches and tags", click "Add deployment
   branch or tag rule" to add a new rule. Set "Ref type" to "Tag" and "Name
   pattern" to `v*`, then click "Add rule"

This allows you to deploy from both the default branch and tags. To configure
GitHub Actions, create the new workflow file `.github/workflows/release.yml`.
The content depends on whether your project has any dependencies or not.

### With dependencies

```yaml
---
name: Release
on:
  workflow_dispatch:
  push:
    tags:
      - 'v*'

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

jobs:
  build:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/inko-lang/idoc:latest
    steps:
      - name: Install dependencies
        run: microdnf install --quiet --assumeyes tar git
      - uses: actions/checkout@v4
      - uses: actions/cache@v4
        with:
          path: '~/.local/share/inko/packages'
          key: deps-${{ hashFiles('inko.pkg') }}
      - name: Install Inko packages
        run: inko pkg sync
      - name: Run tests
        run: inko test
      - name: Build documentation
        run: idoc
      - name: Upload documentation
        uses: actions/upload-pages-artifact@v3
        with:
          path: build/idoc/public

  deploy:
    runs-on: ubuntu-latest
    needs:
      - build
    permissions:
      pages: write
      id-token: write
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - name: Deploy
        id: deployment
        uses: actions/deploy-pages@v4
```

### Without dependencies

```yaml
---
name: Release
on:
  workflow_dispatch:
  push:
    tags:
      - 'v*'

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

jobs:
  build:
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/inko-lang/idoc:latest
    steps:
      - name: Install dependencies
        run: microdnf install --quiet --assumeyes tar git
      - uses: actions/checkout@v4
      - name: Run tests
        run: inko test
      - name: Build documentation
        run: idoc
      - name: Upload documentation
        uses: actions/upload-pages-artifact@v3
        with:
          path: build/idoc/public

  deploy:
    runs-on: ubuntu-latest
    needs:
      - build
    permissions:
      pages: write
      id-token: write
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - name: Deploy
        id: deployment
        uses: actions/deploy-pages@v4
```

### Triggering deployments

With this workflow in place, documentation is built the next time you push a
tag, or by manually triggering the workflow in the "Actions" tab of your
project.
