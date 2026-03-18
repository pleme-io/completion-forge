# completion-forge — Generate Shell Completions from OpenAPI Specs

## Build & Test

```bash
cargo build
cargo test           # 38 tests
cargo run -- generate --spec api.yaml --output ./completions --format all
cargo run -- inspect --spec api.yaml
```

## Architecture

OpenAPI spec → intermediate representation → shell completion files.
Follows the same three-phase pattern as mcp-forge: parse → normalize IR → generate.

### Pipeline

```
OpenAPI 3.0 YAML/JSON
     │
     ▼
spec.rs (minimal serde types: paths, operations, parameters, tags)
     │
     ▼
convert.rs (GroupingStrategy → CommandGroup[] with glyph auto-assignment)
     │
     ├──▶ gen/skim_tab.rs  → skim-tab YAML (commands, icon, subcommands)
     └──▶ gen/fish.rs      → fish completion file (complete -c ... -a ... -d ...)
```

### Module Map

| Module | Purpose |
|--------|---------|
| `src/spec.rs` | Minimal OpenAPI 3.0 serde types (paths, operations, parameters, tags) |
| `src/ir.rs` | `CompletionSpec`, `CommandGroup`, `CompletionOp`, `CompletionFlag`, `Glyph` enum |
| `src/convert.rs` | OpenAPI → IR conversion with `GroupingStrategy` (Auto/ByTag/ByPath/ByOperationId) |
| `src/gen/mod.rs` | Generator dispatcher, `Format` enum (SkimTab/Fish/All) |
| `src/gen/skim_tab.rs` | Generates skim-tab YAML matching `CompletionSpec` serde format |
| `src/gen/fish.rs` | Generates fish completion files with subcommand/flag nesting |
| `src/main.rs` | CLI: generate + inspect subcommands |

### IR Types

```rust
CompletionSpec { name, icon, aliases, description, groups: Vec<CommandGroup> }
CommandGroup   { name, description, glyph: Glyph, operations, flags }
CompletionOp   { name, description, method }
CompletionFlag { name, description, required }
Glyph          { View(◈), Create(◇), Update(↻), Delete(◇), Manage(⊙), Execute(▸), Custom }
```

### Grouping Strategies

| Strategy | Logic |
|----------|-------|
| **Auto** (default) | Tags if present → ByTag; else operationId → ByOperationId; else → ByPath |
| **ByTag** | First OpenAPI tag on each operation → subcommand name |
| **ByPath** | First non-parameter path segment → subcommand name |
| **ByOperationId** | Strip verb prefix (list/get/create/delete) → resource group |

### Glyph Auto-Assignment

Based on HTTP method mix of operations in a group:
- All GET → View (◈)
- All POST → Create (◇)
- All PUT/PATCH → Update (↻)
- All DELETE → Delete (◇)
- Mixed → Manage (⊙)

### CLI

```
completion-forge generate --spec api.yaml --output ./out --format all
completion-forge generate --spec api.yaml --name my-tool --icon "☁" --aliases "mt,tool" --grouping tag
completion-forge inspect --spec api.yaml --grouping auto
```

### Output Formats

**skim-tab YAML** (`{name}.yaml`):
```yaml
commands: [tool-name, alias1]
icon: "☁"
subcommands:
  pets:
    description: "Pet operations"
    glyph: "◈"
```

**fish** (`{name}.fish`):
```fish
complete -c tool -n "__fish_use_subcommand" -a 'pets' -d '◈ Pet operations'
complete -c tool -n "__fish_seen_subcommand_from pets" -l 'limit' -d 'Maximum results'
```

## Design Decisions

- **Minimal OpenAPI types** — only what's needed for completions (no schemas, no auth)
- **Glyph auto-assignment** — matches skim-tab's existing glyph conventions
- **Flag extraction** — path params → required flags, query params → optional, body fields → optional
- **Nix build** — substrate `rust-tool-release-flake.nix` pattern

## Integration

Registered in **forge-gen** as `Completion` category with two targets:
- `skim-tab` — generates YAML for skim-tab's spec registry
- `fish` — generates fish shell completion files

```toml
# forge-gen.toml
[completions]
targets = ["skim-tab", "fish"]
name = "my-tool"
icon = "☁"
grouping = "auto"
aliases = ["mt"]
```
