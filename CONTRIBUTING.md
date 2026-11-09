# Contributing

## Build and run

```sh
cargo run                        # vault in ~/Synapso
SYNAPSO_VAULT=/path cargo run    # or another vault
python fixture/gen_vault.py      # a scratch vault in fixture/vault
```

⚠️ Anything that drives the app — a script, a manual test — points at a scratch
vault with `SYNAPSO_VAULT` and a scratch config with `SYNAPSO_CONFIG`. The app
writes, and ⌘O remembers.

No build step beyond `cargo`. macOS and Windows are the supported platforms:
Linux needs the GTK dev packages `rfd` wants for ⌘O's folder dialog, which this
project doesn't chase. The reason is at the `rfd` line in `Cargo.toml`, next to
the dependency it is about.

## The design source

`design/tokens.css` cannot leave: the `against_the_design` tests in
`crates/synapso-core/src/theme.rs` and `src/theme.rs` `include_str!` it, so the
token tables and the design cannot drift apart, and a missing `include_str!` is
a compile error rather than a skipped test. Nothing else under `design/` is
read by the build.

## The three gates

CI (`.github/workflows/ci.yml`) runs these on every pull request, on GitHub's
hosted **Windows** image — macOS is still the primary target, and the note at
the top of that file says why the gate runs on Windows. Run them yourself before
pushing — the same three, in the same order:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

⚠️ `--workspace` on both, exactly as CI has it. It is the test line that needs
it: without `--workspace`, `cargo test` runs **39** tests instead of **201** —
every test in `crates/synapso-core` is skipped, and the run still says "ok".
(Clippy reaches the core crate either way; the flag is there so the two lines
match what CI runs.)

A PR that fails any of them doesn't merge. `cargo fmt --all` (no `--check`)
fixes the first one for you; the other two you fix by hand.

## The Freya pin

`Cargo.toml` pins `webergnr/freya` by **rev**, and that rev is the head of a
personal branch (`synapso`): upstream Freya plus the commits ADR-009
lists. A force-push to that branch would orphan the commit and break every
checkout that has not vendored it, so tag it on the fork — `synapso-pin-<short>`
— and **move the tag with the pin**: a new rev in `Cargo.toml` needs a new tag
in the same change. All three `freya*` lines move together.

## Citing the design

`design/*.dc.html` is a generated export: every line number in it moves on the
next one. Cite **what** a thing is, not where it sits —
`` `design/Workspace.dc.html`, the status bar's theme slot `` rather than
`` `design/Workspace.dc.html:315` ``. There are still ~60 line-numbered
citations in the tree from before this rule; they are wrong by a dozen lines or
so and are being corrected as each is touched.

## Code style

- Code and comments: **English**, always.
- UI text the user sees: **English** until phase 3 of
  `docs/i18n-trd.md`; migrated strings belong in `crates/synapso-core/src/i18n.rs` so a language
  ships as a complete table instead of a few translated screens.
- Commit messages: one line, lowercase, English — read the `git log` for the
  voice this repo uses.

## Data integrity is the house rule

A failed load must **never** become an eligible save. If a note can't be read,
it opens empty and marked, and saving over it is refused — not silently
patched over. This is the single non-negotiable rule in the codebase; read
`docs/adr-001-integrity-and-boundaries.md` before touching anything in
`vault.rs` or `doc.rs`. If your change adds a new way to read or write a file,
it needs the same refusal-on-failure shape, and a test for it.

## Design decisions

Anything bigger than a bug fix probably has (or needs) an ADR in `docs/`.
Check there first — it's where the "why", not just the "what", lives.

## Pull requests

PRs target `main` and get an automatic Copilot review. Address what it flags
or say why not; either way, don't merge past it silently.
