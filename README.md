# dxpdf-service

**English** | [Русский](README.ru.md)

An HTTP DOCX → PDF conversion service built on the
[dxpdf](https://github.com/nerdy-pro/dxpdf) library, packaged as a native
Windows service (SCM integration via the `windows-service` crate — no
third-party wrappers like NSSM).

## Architecture

```
client ──POST /convert?image-dpi=N──▶ axum (tokio)
                                        │  Semaphore (N = CPUs) — queue for CPU-bound work
                                        ▼
                              spawn_blocking:
                              dxpdf::convert_with_options(bytes, RenderOptions.with_image_dpi(N))
                                        ▼
                              200 application/pdf (PDF bytes)
```

- **One process, no temp files** — dxpdf converts in memory
  (`&[u8] -> Vec<u8>`); the uploaded file never touches disk.
- **Concurrency** — conversion is CPU-bound, so it runs in `spawn_blocking`
  behind a semaphore (default = number of cores); excess requests wait in
  the queue instead of taking the server down.
- **Windows SCM** — the `service` subcommand registers a control handler
  (Stop → graceful axum shutdown) and reports Running/Stopped correctly;
  `install` freezes the settings into the binPath arguments (auto-start,
  LocalSystem).

## HTTP API

| Method | Path | Description |
|---|---|---|
| `POST` | `/convert?image-dpi=300` | Request body — raw .docx bytes; response — `application/pdf` |
| `GET` | `/health` | Liveness probe, answers `ok` |
| `GET` | `/version` | Service version and the exact dxpdf build inside it |

The `image-dpi` parameter (`image_dpi` is accepted too): target resolution
for raster images embedded in the PDF. Range 1–2400, default 220 (matching
Word and the dxpdf CLI).

Response codes:

- `200` — PDF in the response body (`Content-Disposition: attachment`);
- `400` — invalid `image-dpi` or an empty body;
- `413` — body larger than the limit (`--max-body-mb`, default 100 MB);
- `422` — the file does not parse as DOCX (dxpdf's error text in the body);
- `500` — converter panic (the cause is written to the log).

Example:

```bash
curl --data-binary @document.docx "http://192.168.1.33:8080/convert?image-dpi=300" -o document.pdf
```

Prebuilt binaries are on the [Releases](../../releases) page (a zip with
the exe is published by GitHub Actions on every `v*` tag).

## Knowing which build you are running

Three places name the build, because a crash report is only actionable if it
can be tied to one:

- **The log, first line of every run** —
  `dxpdf-service 1.0.2 starting (engine: dxpdf 0.7.0 (https://github.com/ikashapov/dxpdf tag=service-2026-09-11 @ dc33157))`.
  The engine identity comes from `Cargo.lock` at build time, so it names the
  commit a tag resolved to, not just the tag (a tag can be moved).
- **`GET /version`**, and `dxpdf-service.exe --version` — the same string.
  (`-V` stays terse: just the service version.)
- **Windows Application Error events** — the exe carries a VERSIONINFO
  resource, so the event's `version:` field shows `1.0.2.0` rather than
  `0.0.0.0`, and Explorer's *Properties → Details* shows the engine in
  *Comments*. If the build host has no `rc.exe` on PATH the resource is
  skipped with a `cargo:warning`, and the events go back to reporting zeros.

## When the service crashes

A conversion runs C++ (Skia) in-process. A Rust panic there is caught, logged
as `#N panicked` and answered with `500`; a **native** fault — Windows
exception `0xc0000005`, an access violation — is not catchable, and takes the
whole process down with no Rust-level message. The signature is a log with a
`#N start:` line and no matching `#N done`/`failed`/`panicked` line, plus an
Application Error event at the same second.

Work through it in this order — each step splits the problem in half:

1. **Re-run the same document through the CLI on the same machine**
   (`dxpdf.exe problem.docx -o out.pdf`). The CLI is the same engine with no
   HTTP, no tokio and no concurrency. If it also dies, the service is not
   involved at all and the reproduction is a one-liner to attach to a bug.
2. **Check the Visual C++ runtime on the host.** The faulting module in the
   event is often `MSVCP140.dll`; its `version:` field is the redistributable
   actually installed. The exe links it dynamically, so a host whose redist
   is years older than the toolset the exe was built with is a real
   suspect — install the current *Microsoft Visual C++ 2015–2022
   Redistributable (x64)* and retry before digging further.
3. **Take concurrency out.** Reinstall with `--concurrency 1`: one conversion
   at a time. If the crashes stop, the cause is parallel use of a shared
   resource rather than the document itself.
4. **Get a stack.** Have Windows Error Reporting keep a dump, then re-run:

   ```bat
   reg add "HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\dxpdf-service.exe" /v DumpFolder /t REG_EXPAND_SZ /d C:\dumps /f
   reg add "HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting\LocalDumps\dxpdf-service.exe" /v DumpType /t REG_DWORD /d 2 /f
   ```

   The `.dmp` that appears in `C:\dumps` after the next crash carries the
   faulting stack — open it in WinDbg or Visual Studio, or attach it to the
   report.
5. **Raise the log level** for the run that reproduces it:
   `set RUST_LOG=debug`. dxpdf then logs its per-phase timing and the
   resolution decision for every requested font family, so the last line
   before the process dies names the phase — and, in the font phase, the
   family — it died in.

## Which dxpdf this builds against

`Cargo.toml` pins the engine by **git tag**, not by a path:

```toml
dxpdf = { git = "https://github.com/ikashapov/dxpdf", tag = "service-2026-09-11" }
```

A tag rather than a branch so the build is reproducible: a branch would be
re-resolved on every `cargo update` and the service would silently change
engines. A git dependency rather than `path = "../dxpdf"` because that
checkout moves between feature branches, and the service was once built from
whatever happened to be checked out there.

`service-*` tags point at the fork's `integration/*` branch — upstream
`nerdy-pro/dxpdf` plus the feature branches that have not been merged
upstream yet. Two things that matter operationally were fixed this way:

- **`unreachable: caller only passes XML whitespace`** — a panic that failed
  21 conversions in production, fixed upstream in v0.6.0;
- **Strict Open XML uploads** (`<w:pgSz w:w="595.30pt"/>`) — rejected with
  *"expected an integer or decimal measurement"* until the universal-measure
  parse landed on the fork.

To build against upstream only, swap the line for
`{ git = "https://github.com/nerdy-pro/dxpdf", tag = "v0.7.0" }` — you lose
the Strict-document support and the unmerged features. For local engine work
use `{ path = "../dxpdf" }`, but do not commit it.

Bumping the engine:

```powershell
cargo update -p dxpdf   # after editing the tag in Cargo.toml
cargo build --release
```

## Building (on Windows)

Requirements: rustup (MSVC toolchain) and VS Build Tools. `skia-safe`
downloads prebuilt Skia binaries — clang/python are not needed. The first
build fetches the engine and compiles Skia, so allow ~10 minutes.

```powershell
cd dxpdf-service
cargo build --release
```

The `../dxpdf` folder is no longer required — the engine comes from git.

## Install / operate

```powershell
# copy the exe to a stable location (not target\ — a rebuild would lock the file)
Copy-Item target\release\dxpdf-service.exe C:\svc\bin\

# install + start (auto-start at boot, LocalSystem)
C:\svc\bin\dxpdf-service.exe install --port 8080 --log-file C:\svc\dxpdf-service.log

# manage
Restart-Service DxPdfService
Stop-Service DxPdfService
C:\svc\bin\dxpdf-service.exe uninstall

# console debugging (no SCM, Ctrl+C stops)
dxpdf-service.exe run --port 8091
```

`install`/`run`/`service` flags: `--host` (0.0.0.0), `--port` (8080),
`--max-body-mb` (100), `--concurrency` (0 = number of CPUs), `--log-file`
(in service mode defaults to `dxpdf-service.log` next to the exe).
Log level — the `RUST_LOG` variable (default `info`).

If the service stops immediately with Event ID 7024 "Incorrect function"
(service-specific error 1) — the HTTP server failed to start; almost always
the port is already in use. Check the log file for the cause, find the
occupying process with `netstat -ano | findstr :8080`, or reinstall the
service on a different port.

External access needs an inbound firewall rule:

```powershell
New-NetFirewallRule -DisplayName 'DxPdfService HTTP 8080' -Direction Inbound -Protocol TCP -LocalPort 8080 -Action Allow
```

## Upgrading

```powershell
Stop-Service DxPdfService
cargo build --release
Copy-Item target\release\dxpdf-service.exe C:\svc\bin\ -Force
Start-Service DxPdfService
```
