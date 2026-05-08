# datastar-lsp

Language server for [Datastar](https://data-star.dev) hypermedia framework.

**Features:**
- Diagnostics: unknown attributes, missing keys/values, undefined signals/actions, invalid modifiers
- Hover: documentation for attributes, signals, actions, modifiers
- Completions: `$` signals, `@` actions, `:` keys, `__` modifiers
- Go-to-definition: `$signal` → `data-signals:signal` (cross-file)
- Find references: all `$signal` usages across open files
- Rename: rename signal across all open files
- Code actions: define undefined signals, add missing values/keys

**Languages:** HTML, Templ (Go), JSX, TSX, HEEx (Elixir), Blade (PHP)

## Install

### Neovim (lazy.nvim)

```lua
{
    "hyperpuncher/datastar-lsp",
    opts = {},
}
```

### Manual

Download binary from [releases](https://github.com/hyperpuncher/datastar-lsp/releases), configure your editor:

```lua
-- Neovim 0.11+
vim.lsp.config("datastar_ls", {
    cmd = { "/path/to/datastar-lsp" },
    filetypes = { "html", "templ", "heex", "blade", "javascriptreact", "typescriptreact" },
    root_markers = { ".git" },
})
vim.lsp.enable("datastar_ls")
```

### Build from source

```bash
cargo install --git https://github.com/hyperpuncher/datastar-lsp
```

## Requirements

- [tree-sitter-datastar](https://github.com/hyperpuncher/tree-sitter-datastar) for syntax highlighting (optional but recommended)
