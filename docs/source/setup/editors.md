---
{
  "title": "Setting up your editor"
}
---

## Emacs

No official plugin exists for Emacs, but a syntax definition for Inko is [found
in this discussion](https://github.com/orgs/inko-lang/discussions/697).

## (Neo)Vim

For Vim and Neovim we provide [an official
plugin](https://github.com/inko-lang/inko.vim). This plugin adds support for
syntax highlighting, file type detection, folding, and indentation.

The plugin [conform.nvim](https://github.com/stevearc/conform.nvim) adds support
for automatic formatting of source code using `inko fmt`.

## Visual Studio Code

An official extension for Visual Studio Code is provided
[here](https://marketplace.visualstudio.com/items?itemName=inko-lang.inko). To
install it, open VS Code's Quick Open window (Ctrl+P) and run the following:

```
ext install inko-lang.inko
```
