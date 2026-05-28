# datastar-lsp

Language server for [Datastar](https://data-star.dev) hypermedia framework.

## Features

- **Diagnostics** - unknown attributes, missing keys/values, undefined signals/actions, invalid modifiers
- **Hover** - documentation for attributes, signals, actions, and modifiers
- **Completions** - `$` signals, `@` actions, `:` keys, `__` modifiers
- **Go-to-definition** - `$signal` → `data-signals:signal` (cross-file)
- **Find references** - all `$signal` usages across open files
- **Rename** - rename signal across all open files
- **Code actions** - define undefined signals, add missing values/keys

## Supported Languages

HTML · Templ (Go) · JSX · TSX · HEEx (Elixir) · Blade (PHP)

## Install

### Neovim (lazy.nvim)

```lua
{
    "hyperpuncher/datastar-lsp",
    opts = {},
}
```

### VS Code

Download `datastar-lsp-*.vsix` from the [latest release](https://github.com/hyperpuncher/datastar-lsp/releases/latest), then:

```bash
code --install-extension datastar-lsp-*.vsix
```

### Zed

```bash
git clone https://github.com/hyperpuncher/datastar-lsp \
  ~/.config/zed/extensions/datastar
```

Then in Zed, run `zed: install cli` and `zed: reload extensions`.

## Requirements

- [tree-sitter-datastar](https://github.com/hyperpuncher/tree-sitter-datastar) for syntax highlighting (optional but recommended)
