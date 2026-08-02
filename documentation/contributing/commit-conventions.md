# Commit conventions

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

## Types

| Type       | Use for |
|------------|---------|
| `feat`     | a new feature |
| `fix`      | a bug fix |
| `docs`     | documentation only |
| `style`    | formatting, no code meaning change (rustfmt, whitespace) |
| `refactor` | code change that neither fixes a bug nor adds a feature |
| `perf`     | a performance improvement |
| `test`     | adding or correcting tests |
| `build`    | build system or dependencies (`Cargo.toml`, workspace setup) |
| `ci`       | devcontainer, CI configuration |
| `chore`    | anything else that doesn't fit the above |
| `revert`   | reverting a previous commit |

## Scope

Optional, in parentheses right after the type. Use the crate or area touched, e.g. `simlogix-core`, `simlogix-gui`, `devcontainer`, `docs`. Omit it when the change is repo-wide or doesn't map to one area.

## Breaking changes

Add `!` after the type/scope (`feat(simlogix-core)!: ...`) and/or a `BREAKING CHANGE:` footer describing the impact.

## Examples

```
feat(simlogix-core): add Signal enum
fix(simlogix-gui): correct window title
docs: add commit conventions
ci(devcontainer): add libxkbcommon-x11-dev
chore: replace serayuzgur.crates with fill-labs.dependi
```

## Enforcement

A `commit-msg` git hook in [.githooks/commit-msg](../../.githooks/commit-msg) checks the subject line against this format and rejects the commit otherwise. It's wired up automatically when the devcontainer is created (`postCreateCommand` runs `git config core.hooksPath .githooks`); if you're committing from outside the devcontainer, run that command once yourself.
