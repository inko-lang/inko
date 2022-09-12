# Editor integration

## Vim/NeoVim

For Vim and Neovim we provide [an official
plugin](https://gitlab.com/inko-lang/inko.vim). This plugin adds support for
syntax highlighting, file type detection, folding, and indentation.

To use this plugin, add the following to your `.vimrc` or `init.lua` (if you're
using NeoVim):

=== "vim-plug"
    ```vim
    Plug 'https://gitlab.com/inko-lang/inko.vim.git'
    ```
=== "packer.nvim"
    ```lua
    use 'https://gitlab.com/inko-lang/inko.vim.git'
    ```
