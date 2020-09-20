# Text editor setup

Inko does not require you to use a special IDE. Instead, you can use whatever
text editor or IDE you prefer. On this page you can find various plugins that
add support for Inko to various text editors/IDEs.

If there is no Inko support for your editor, don't worry. Inko source code is
simple enough to read, even without syntax highlighting. Just make sure you use
the right amount of indentation: 2 spaces, no tabs. For more information about
this, refer to the [Style guide](../style-guide.md).

## Vim

We provide an official [Vim plugin for
Inko](https://gitlab.com/inko-lang/inko.vim). This plugin provides support for
syntax highlighting, indentation, folding, and more.

Using [vim-plug](https://github.com/junegunn/vim-plug) you can install it by
adding the following to your Vim configuration:

```vim
Plug 'https://gitlab.com/inko-lang/inko.vim.git'
```

Then restart Vim, and run `:PlugInstall` to install the plugin.
