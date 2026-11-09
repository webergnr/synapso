# TRD — Multi-language UI (i18n)

**Date:** 2026-08-17 · **Status:** phase 1 built; phases 2–4 pending ·
**Context:** the repo is going open source and the interface should speak more than one language. Written
against `main@ca532bd` (numbers below are counted, not estimated).

## Open questions for Weber (pinned — answer whenever, the rest can be built)

1. **Where does the chosen language live?** Today the only preferences store is
   `.synapso.json` **inside the vault**, so a second vault forgets your
   language. [ASSUMED: language is a *user* setting, not a vault setting, so
   this TRD proposes the app's own config file — the first one this project
   would have.] Say the word and it goes in the vault sidecar instead (one line,
   and no new file).
2. **Which languages ship first?** [ASSUMED: `en` as source + `pt-BR`, because
   those are the two you already write.] A third one changes nothing in the
   design; it changes the review burden.
3. **Do translations have to ship without a rebuild?** Compile-time tables are
   cheaper and safer (§4); runtime `.ftl`/`.json` files let a translator who
   cannot build Rust contribute. [ASSUMED: compile-time now, runtime deferred
   with a trigger.]
4. **Are the theme names translated?** *Syn Dark* / *Syn Light* read like
   product names. [ASSUMED: not translated.]

## 1. What is in scope

Everything the app says to a person: the status bar, the ⌘K palette, the ⌘P
picker, modals, tooltips, context menus, the header, the empty states.

**Out of scope, and it matters that they stay out:**

| Not translated | Why |
| --- | --- |
| Note and canvas content | It is the user's file; the app has no opinion about its language |
| Markdown input rules (`- `, `1. `, `> `, ` ``` `) | They are file syntax, not prose. A localized `- ` would make notes unportable |
| JSON Canvas keys (`nodes`, `edges`, `shape`, `fontSize`) | Interop with Obsidian; the spec is the spec |
| Keyboard glyphs (`⌘K`, `⇧Tab`) and tool letters (`V R O Y A L P T I F`) | They are what is printed on the keyboard, not words |
| Slug/sanitization behaviour (`vault::sanitize`) | Already language-agnostic — it folds accents (`maçã` → `maca`) and dodges Windows reserved names |
| `Syn Dark` / `Syn Light` | Product names (see open question 4) |
| Log/bench output (`src/bench.rs`) | Developer surface |

## 2. Census — what actually has to move

Counted on `main@ca532bd`:

| Where | Count | Shape |
| --- | --- | --- |
| `src/main.rs` | 57 | status-bar messages (28 `status =` sites), modal titles, `format!` with prose |
| `src/editor.rs` | 12 | header, toolbar tooltips, context-menu rows |
| `src/picker.rs` | 9 | scope labels (`recent` / `on canvas` / `all`), badges, empty state |
| `src/palette.rs` | 5 + table | the command table itself: 16 rows of `("doc", "new note", "⌘N", Command::NewNote)` |
| `src/board.rs` | 5 | canvas empty state, card subtitles |
| **Total** | **≈90 strings** | of which ~30 carry a value (`format!`) |

Three of those are not plain strings and are the real work:

- ⚠️ **`pub fn plural(n, word) -> String` (`main.rs:1406`) hardcodes English
  morphology** — `format!("{n} {word}{}", if n == 1 {""} else {"s"})`. Three
  callers (`palette.rs:246`, `palette.rs:486`, `board.rs:1589`). Portuguese
  happens to also be `+s`, which is exactly the trap: the function will look
  fine in the second language and break in the third.
  The code already knows this, in one place: `Find::summary` (`find.rs:195-203`)
  spells `no matches` / `1 match` / `n matches` by hand, with the comment
  *"`matchs` [sic] is what `plural` would produce, and English disagrees"*. That is the
  helper failing on its **own** language — a per-language function is not
  over-engineering, it is what the second exception already proves.
- **Composed status messages.** The refusal paths concatenate:
  `format!("{name} not created — {}", s.status)`. The order matters because the
  status bar truncates at the *end*, so the sentence's own head has to carry the
  news. A translation cannot flip that order without losing the message.
- **The command table** is `(kind, label, accel, Command)`. `kind` (`doc`,
  `view`, `canvas`, `vault`, `theme`) is displayed as a category chip, so it is
  a string that is also a grouping key — those two roles have to split.

## 3. What the tests do today

⚠️ **Eleven assertions compare against English UI text**
(`h.state.status.contains("line of text")`, `starts_with("beta left unsaved")`,
`"changed on disk"`). Any i18n mechanism that can change what `status` holds at
runtime invalidates them. This is the biggest hidden cost of the work, and it is
also the opportunity: after the migration, those tests assert against
`i18n::en().only_text_becomes_task` and stop being string-typo-sensitive.

The test harness must pin the language (`Lang::En`) so a contributor's OS locale
cannot turn the suite red.

## 4. Design

### 4.1 The table, not a lookup

Follow what the repo already does for themes (`theme::THEMES`: a table, and
adding one is a line that shows up in ⌘K by itself). `src/i18n.rs`:

```rust
pub struct Strings {
    pub new_note: &'static str,
    pub reload_vault: &'static str,
    // …and for anything with a value, a function instead of a template:
    pub saved: fn(&str) -> String,           // "saved {name}"
    pub not_created: fn(&str, &str) -> String,
    pub words: fn(usize) -> String,          // plural lives per language
}

pub const LANGS: &[(&str, &str, Strings)] = &[
    ("en", "English", EN),
    ("pt-BR", "Português (Brasil)", PT_BR),
];
```

Why a struct of fields and fn pointers rather than a key→string map:

- **The compiler is the completeness check.** A new language that forgets a
  string does not compile. No missing-key-at-runtime, no `t!("typo.key")`
  silently rendering the key.
- **Arity and types are checked too.** `saved: fn(&str) -> String` cannot be
  called with the wrong number of arguments; a `"{}"` template can.
- **Plurals belong to the language, not to a helper.** `words` is a field, so
  Portuguese can say `1 palavra` / `2 palavras` and a future Slavic language can
  do three forms in its own function without touching the other languages.
  ⚠️ `crate::plural` gets deleted, not localized.
- No new dependency. `rust-i18n` and `fluent` both earn their weight later
  (§6) — neither earns it for 90 strings and two languages.

### 4.2 Reaching the views

Themes are threaded (`&Tokens` into every view function). Threading a second
parameter through every signature is a large mechanical diff for something that
changes maybe twice a session, so instead:

```rust
static LANG: AtomicUsize = AtomicUsize::new(0);

pub fn s() -> &'static Strings { LANGS[LANG.load(Ordering::Relaxed)].2 }

/// ⚠️ Out of range picks English instead of panicking: the index comes from a
/// config file or an OS locale — somebody else's data — and `s()` indexes the
/// table directly on every frame.
pub fn set(i: usize) {
    LANG.store(if i < LANGS.len() { i } else { 0 }, Ordering::Relaxed)
}
```

Call sites read `i18n::s().new_note`. iced rebuilds the view every frame, so a
language change repaints with no restart and no invalidation bookkeeping.
[ASSUMED: a global is acceptable *here* because the app is one window and one
user; the theme stays threaded because it is already threaded and per-widget.]

⚠️ Tests run in parallel in one process, so `LANG` is process-wide state exactly
like `SYNAPSO_VAULT` — the harness already holds a `Mutex` for that; the
language pin goes under the same lock.

### 4.3 Picking the language

1. The app config value, if set (open question 1).
2. Otherwise the OS locale, matched loosely (`pt_BR.UTF-8` → `pt-BR`, `pt-PT` →
   `pt-BR` with a comment saying why, `de-CH` → `en` until German exists).
   `sys-locale` is ~40 lines of platform glue; [ASSUMED: worth the dependency,
   because the alternative is every non-English user starting in English].
3. Otherwise `en`.

Switching: a ⌘K row per language, exactly like the theme rows
(`Command::Lang(i)`), and the footer control that shows the theme gets the
language beside it. No restart.

### 4.4 The one enforcement that keeps it done

A test that walks `src/*.rs` (excluding `i18n.rs`, tests and `bench.rs`) and
fails on a user-facing literal: any `status = "…"`, `status = format!("…"` or
`text("…")` whose content has two or more ASCII letters and is not a glyph.
Same spirit as the existing invariant *"no widget uses a colour outside
`theme.rs`"* — and the reason that invariant survived is that it is checked, not
just written down.

## 5. Phases

| Phase | Scope | Done when |
| --- | --- | --- |
| **1 — the seam** | `src/i18n.rs` with `Strings`, `EN`, `LANGS`, `s()`, `set()`; delete `crate::plural`; migrate the 28 status sites and the 11 tests that read them | `cargo test` green with every assertion going through `i18n::en()`, zero behaviour change, one language |
| **2 — the rest of the surface** | palette command table (split `kind` from its label), picker scopes/badges, editor header + tooltips + context menu, board empty states, modals | The literal-sweep test from §4.4 passes with no allowlist |
| **3 — second language** | `PT_BR`, OS-locale detection, ⌘K rows, config persistence | Launching with `SYNAPSO_LANG=pt-BR` comes up in Portuguese; switching in ⌘K repaints without restart; the vault is untouched by the switch |
| **4 — the layout truth** | A pseudo-locale (`SYNAPSO_LANG=xx`, the same override as phase 3 → every string wrapped and 35% longer) and a pass over what clips | Status bar, ⌘P rows, ⌘K rows, tooltips and the **vertical rail** survive the pseudo-locale without an unreadable clip |

Phase 4 is not decoration: German and Portuguese run 20–35% longer than
English, the rail draws **rotated text** in a fixed-width column, and the status
bar already truncates at the end. Those three are where a translation looks
broken.

## 6. Deferred, with triggers

| Deferred | Why | Trigger |
| --- | --- | --- |
| **Fluent / ICU MessageFormat** | Real plural categories, gender, ordered arguments in translator-controlled order | The first language with more than two plural forms (ru, pl, ar), or the first string a translator cannot fix without reordering arguments |
| **Runtime-loaded translations** (`.ftl`/`.json` in the vault or next to the binary) | Lets a non-Rust translator ship a language | A translator outside the contributor set, or wanting a language without cutting a release |
| **RTL (ar, he)** | iced 0.14 has no bidi layout; mirroring the whole shell is a design job, not a string job | Someone asks — and then it is a design ADR, not this TRD |
| **Localized number/date formats** | The only numbers shown are integers (word counts, zoom %, `2 of 2`) | The first decimal or date on screen (`1,5` vs `1.5` is a real difference in pt-BR) |
| **Translated docs (README/ADRs)** | English is the lingua franca of the repo; a stale translated README is worse than none | If a language community forms around it — then `README.pt-BR.md`, never a fork of the English one |

## 7. Risks

- **The second language hides the third's bugs.** pt-BR pluralizes like English
  and reads left-to-right — Phase 3 will pass while `plural`-shaped assumptions
  survive elsewhere. The mitigation is the pseudo-locale in Phase 4, which is
  deliberately hostile in ways pt-BR is not.
- **Composed messages.** The refusal chain (`"{name} not created — {reason}"`)
  is two translated halves in a fixed order. If a language needs the reason
  first, the *function* has to build the whole sentence, not glue two strings.
  The fn-pointer design allows that; a template map would not.
- **The test suite is the surface most coupled to English.** Migrating it is
  Phase 1's actual work, not the new module.
- **Contributor confusion about what is translatable.** §1's table goes into
  `CONTRIBUTING.md`, which PR #55 added — the table is a section to append,
  not a file to create.
