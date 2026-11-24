---
title: "Editor Configuration Recommendations"
status: CURRENT
last_updated: 2026-02-02
category: reference
---

# Editor Configuration Insights from Doom Emacs Config

## What We Can Learn

Based on [Adam Zaninovich's Doom Emacs config](https://github.com/adamzaninovich/doom-emacs-config/blob/main/config.org#setup-ploymode-with-elixir-and-web-mode), here are key improvements we could adopt:

### 1. Auto-Format on Save

Their config runs `elixir-format` automatically on save:

```elixir
(add-hook 'elixir-mode-hook
          (lambda ()
            (add-hook 'before-save-hook 'elixir-format nil t)))
```

**Current State**: We have `.formatter.exs` files but no auto-format on save configured

**Recommendation**: Add to `.vscode/settings.json` or create `.editorconfig` entry

### 2. LSP File Watcher Configuration

They disable LSP file watchers to avoid performance issues:

```elixir
(setq lsp-enable-file-watchers nil)
```

**For AllSource**: We have large codebases (core, query-service, mcp-server). Consider:
- Excluding `node_modules`, `_build`, `target`, `deps` from LSP watching
- Configure `.elixir_ls/` directory ignores

### 3. Format Keybindings

They bind format to localleader for easy access:

```elixir
(map! :after elixir-mode
      :map elixir-mode-map
      :localleader
      :n "f" #'elixir-format)
```

**For VS Code**: Can add keybinding in `.vscode/keybindings.json`

### 4. Polymode for LiveView Templates (Future)

If you plan to add Phoenix LiveView to the stack, the polymode setup enables proper syntax highlighting inside `~H"""` sigils:

```elixir
# In Elixir file, LiveView template gets web-mode highlighting
def render(assigns) do
  ~H"""
  <div class="container">
    <h1><%= @title %></h1>
  </div>
  """
end
```

**Current Need**: Not applicable yet - we're using Phoenix API only, no LiveView

## Recommendations for AllSource

### Immediate Actions

1. **Create `.vscode/settings.json`** (if using VS Code):
```json
{
  "[elixir]": {
    "editor.formatOnSave": true,
    "editor.defaultFormatter": "jakebecker.elixir-ls"
  },
  "files.watcherExclude": {
    "**/_build/**": true,
    "**/deps/**": true,
    "**/node_modules/**": true,
    "**/target/**": true,
    "**/.elixir_ls/**": true
  },
  "elixirLS.projectDir": "apps/query-service",
  "files.exclude": {
    "**/_build": true,
    "**/deps": true,
    "**/.elixir_ls": true
  }
}
```

2. **Add `.editorconfig`** at root:
```ini
[*.{ex,exs}]
indent_style = space
indent_size = 2
end_of_line = lf
charset = utf-8
trim_trailing_whitespace = true
insert_final_newline = true

[*.{rs}]
indent_style = space
indent_size = 4

[*.{go}]
indent_style = tab
indent_size = 4
```

3. **Configure LSP ignores**:
   - Add to each Elixir project's `.elixir_ls/` ignore patterns
   - Or configure in `elixirLS.settings` to exclude build artifacts

### Current State

✅ We have `.formatter.exs` configured
✅ Formatter settings are consistent across projects
❌ No auto-format on save configured
❌ No LSP ignore patterns configured
❌ No editor-specific configs documented

### Key Takeaways

1. **Auto-format on save** = Consistency without thinking
2. **LSP exclusions** = Better performance in monorepo
3. **Polymode** = Useful if/when adding LiveView templates
4. **Format keybindings** = Quick access to formatting

## Next Steps

1. Create `.vscode/settings.json` with Elixir-specific settings
2. Add `.editorconfig` for consistent formatting across editors
3. Document editor setup in README or CONTRIBUTING.md
4. Add LSP exclusions for performance

