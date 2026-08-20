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

## Building (on Windows)

Requirements: rustup (MSVC toolchain) and VS Build Tools. `skia-safe`
downloads prebuilt Skia binaries — clang/python are not needed.

```powershell
cd dxpdf-service
cargo build --release   # the ../dxpdf folder must sit next to this one
```

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
