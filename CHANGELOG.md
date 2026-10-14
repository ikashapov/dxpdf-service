# Changelog

Each section is the body of the matching GitHub release: the Release workflow
copies the section whose heading names the pushed tag (`## v1.0.4 (...)`) into
the release and refuses to publish a tag that has none.

## v1.0.4 (2026-10-14)

The service itself is unchanged in shape: same HTTP API, same Windows service wrapper, same `/convert` and `/version` endpoints. What changed is the engine underneath and how much the service logs per conversion.

Covers everything since v1.0.2; 1.0.3 was the same engine built before its tag was re-pointed (see "About 1.0.3" below).

Compare: [v1.0.2...v1.0.4](https://github.com/ikashapov/dxpdf-service/compare/v1.0.2...v1.0.4) · engine [service-2026-09-11...service-2026-10-14](https://github.com/ikashapov/dxpdf/compare/service-2026-09-11...service-2026-10-14)

### Engine: `service-2026-09-11` → `service-2026-10-14`

v1.0.2 built against the fork tag `service-2026-09-11`: upstream dxpdf v0.7.0 plus ten fork branches. This release builds against `service-2026-10-14`: upstream dxpdf `main` as of 22 September 2026 (v0.8.0, v0.8.1 and 47 merged pull requests) plus the ten fork pull requests still open upstream, integrated as one commit.

#### New since v1.0.2, from the fork (open upstream PRs)

- **Five layout defects found against Word in the `examples/` corpus** ([#252](https://github.com/nerdy-pro/dxpdf/pull/252), closes #228–#232). A bookmark-only paragraph no longer takes a line of its own, so a document that ended with one no longer gains a blank page. A nested table is sized against its cell, not the page. Two lists over one abstract numbering share one sequence, and `w:startOverride` fires once. Table widths resolve the way Word does: percent widths relative to the container, autofit tables from `w:tcW`. Columns closed by a continuous section break are balanced, and the `w:sep` separator line is drawn.
- **Automatic hyphenation** ([#259](https://github.com/nerdy-pro/dxpdf/pull/259), issue #158). `w:autoHyphenation` is honoured with `w:hyphenationZone`, `w:consecutiveHyphenLimit`, `w:doNotHyphenateCaps` and the paragraph-level `w:suppressAutoHyphens`; the cut point follows the run's `w:lang`. Dictionaries for en-US, en-GB, German and Russian are built in. Documents without the setting render byte-identically to before.
- **Missing fonts no longer slow the conversion down** ([#258](https://github.com/nerdy-pro/dxpdf/pull/258), issue #188). A document naming a family absent from the host used to make the engine open every host face before consulting its substitution table, on every conversion. The substitution is now answered first, probes are memoised, and theme script fonts are no longer preloaded.
- **Pictures with hyperlinks** ([#257](https://github.com/nerdy-pro/dxpdf/pull/257), issue #250). `a:hlinkClick` on an inline or floating picture becomes a link annotation over the image in the PDF.
- **Tracked deletion of a field code** ([#256](https://github.com/nerdy-pro/dxpdf/pull/256), issue #253). `w:delInstrText` is accepted instead of failing the parse; the deleted field code is never drawn.

#### Carried over from v1.0.2

Five of the ten branches in `service-2026-09-11` have since been merged upstream and stay in this build from there: text effects (#182), bidirectional tabs (#181), cell shading patterns (#180), counting systems (#178) and Strict Open XML universal measures (#200). The other five are still fork pull requests and are included unchanged: WMF/EMF and SVG images (#183), page borders and document grid (#184), SmartArt and charts (#185), shaped text clusters (#186), tracked changes and comments (#187).

#### Upstream changes picked up (v0.7.0 → `main` at 3a5c7ca)

- Shading patterns: #180 and its follow-ups #223–#227, #233–#236 (stripe geometry confirmed against Word, restart at page splits, adjacency seams, a cap on stripe count).
- Counting systems: #178 with #220–#222 (Hindi crore overflow, the Thai เอ็ด rule, numeral fixes).
- Bidirectional text: #181 with #237–#246 (tab placement, decimal-tab anchoring, mirrored pen placement, visual-order checks).
- Text effects: #182 with #247–#249 and #251 (effects persist across line splits, footnote-number prefix inheritance, shadow and outline emission).
- Math: #201–#208 (fraction geometry, superscripts over fraction bases, the opening equation's font for footnote prefixes, float checkpoint state, no blank pages from indentation).
- Strict Open XML measures and percent spellings: #200, #209.
- Hyphens next to non-ASCII digits: #202.
- Releases and bindings: v0.8.0 (#210), v0.8.1 (#217); Go bindings for windows/amd64 (#213–#218); the Python binding releases the GIL during conversion (#211); README feature-matrix consistency checks (#240).

### Service

- **Quieter default log.** The default filter is now `info` for the service and `error` for the engine's font resolver and VML style parser (`DEFAULT_LOG_FILTER` in `main.rs`). The `[font] … falling back to the host default` lines that appeared on every conversion are gone from the default log. Setting `RUST_LOG=info` restores them; `RUST_LOG=debug` adds the catalogue details.
- **README (en/ru)** documents the log levels and which engine tag the build is pinned to.

### About 1.0.3

1.0.3 shipped this same engine source tree, built while the fork tag still pointed at an earlier replay of the integration branch (`e9bd2d5`). That binary reports `engine: … tag=service-2026-10-14 @ e9bd2d5`, a commit the fork no longer carries, and its bundled README cites a third SHA. 1.0.4 is the rebuild after the tag was re-pointed to the squashed integration commit `3637735`: the identity string, the README and the tag now agree. Conversions are identical between 1.0.3 and 1.0.4.

### Also in 1.0.4

- The log-filter constant is wrapped the way rustfmt wants, which is what made the Windows CI job fail on 1.0.3.
- The CI and Release workflows no longer check out a stale sibling `nerdy-pro/dxpdf` at `b2c0174`; cargo fetches the engine from the tag in Cargo.toml.

## v1.0.3 (2026-10-14)

The engine move to `service-2026-10-14` and the quieter default log described
under v1.0.4 first shipped here. The binary was built while the fork tag still
pointed at an earlier replay of the integration branch, so it reports
`engine: … @ e9bd2d5`, a commit the fork no longer carries; conversions are
identical to v1.0.4. The Windows CI job failed on `cargo fmt --check` for this
commit.

Compare: [v1.0.2...v1.0.3](https://github.com/ikashapov/dxpdf-service/compare/v1.0.2...v1.0.3)

## v1.0.2 (2026-09-11)

- Three places now name the build, so a crash report can be tied to one: the
  first log line of every run, `GET /version` and `--version`, and the exe's
  version resource. The engine identity (repository, tag and the commit the
  tag resolved to) is read from `Cargo.lock` at build time.
- README (en/ru): what to send with a crash report, and where the
  `0xc0000005` in `MSVCP140.dll` during font resolution stands.
- Fixes the Windows clippy job (an unused import).

Compare: [v1.0.1...v1.0.2](https://github.com/ikashapov/dxpdf-service/compare/v1.0.1...v1.0.2)

## v1.0.1 (2026-09-11)

- The engine is a git dependency pinned by tag (`service-2026-09-11` on
  `ikashapov/dxpdf`) instead of `path = "../dxpdf"`, so a build no longer
  depends on whatever that checkout happened to be on.

Compare: [v1.0.0...v1.0.1](https://github.com/ikashapov/dxpdf-service/compare/v1.0.0...v1.0.1)

## v1.0.0 (2026-08-20)

First release: an HTTP service, installable as a Windows service, converting
DOCX to PDF with dxpdf. CI builds and tests on every push; a `v*` tag publishes
a release with the Windows binary. README in English, with the Russian
original kept as README.ru.md.
