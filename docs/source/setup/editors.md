---
{
  "title": "Setting up your editor"
}
---

## (Neo)Vim

For Vim and Neovim we provide [an official
plugin](https://github.com/inko-lang/inko.vim). This plugin adds support for
syntax highlighting, file type detection, folding, and indentation.

### vim-plug

```vim
Plug 'inko-lang/inko.vim'
```

### packer.nvim

```lua
use 'inko-lang/inko.vim'
```

## Visual Studio Code

An official extension for Visual Studio Code is provided
[here](https://marketplace.visualstudio.com/items?itemName=inko-lang.inko). To
install it, open VS Code's Quick Open window (Ctrl+P) and run the following:

```
ext install inko-lang.inko
```
