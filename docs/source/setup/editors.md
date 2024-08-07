---
{
  "title": "Setting up your editor"
}
---

## Emacs

No official plugin exists for Emacs, but a syntax definition for Inko is [found
in this discussion](https://github.com/orgs/inko-lang/discussions/697).

## Helix

Starting with version 24.07, [Helix](https://helix-editor.com/) has built-in
support for syntax highlighting and code formatting, thanks to the official
[Tree-sitter grammar for Inko][ts-grammar].

## (Neo)Vim

For Vim and Neovim we provide [an official
plugin](https://github.com/inko-lang/inko.vim). This plugin adds support for
syntax highlighting, file type detection, folding, and indentation.

Users of NeoVim can also use
[nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) to take
advantage of [Tree-sitter support for Inko][ts-grammar], providing more accurate
highlights.

The NeoVim plugin [conform.nvim](https://github.com/stevearc/conform.nvim)
provides support for automatic formatting of source code using `inko fmt`.


## Visual Studio Code

An official extension for Visual Studio Code is provided
[here](https://marketplace.visualstudio.com/items?itemName=inko-lang.inko). To
install it, open VS Code's Quick Open window (Ctrl+P) and run the following:

```
ext install inko-lang.inko
```

[ts-grammar]: https://github.com/inko-lang/tree-sitter-inko/
