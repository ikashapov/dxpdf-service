# dxpdf-service

[English](README.md) | **Русский**

HTTP-сервис конвертации DOCX → PDF на базе библиотеки
[dxpdf](https://github.com/nerdy-pro/dxpdf),
оформленный как нативный Windows-сервис (интеграция с SCM через крейт
`windows-service`, без сторонних обёрток типа NSSM).

## Архитектура

```
клиент ──POST /convert?image-dpi=N──▶ axum (tokio)
                                        │  Semaphore (N = CPU) — очередь CPU-bound задач
                                        ▼
                              spawn_blocking:
                              dxpdf::convert_with_options(bytes, RenderOptions.with_image_dpi(N))
                                        ▼
                              200 application/pdf (байты PDF)
```

- **Один процесс, без временных файлов** — dxpdf работает по байтам в памяти
  (`&[u8] -> Vec<u8>`), файл из тела запроса не касается диска.
- **Конкурентность** — конвертация CPU-bound, поэтому выполняется в
  `spawn_blocking` под семафором (по умолчанию = число ядер); лишние запросы
  ждут в очереди, а не роняют сервер.
- **Windows SCM** — `service`-подкоманда регистрирует control-handler
  (Stop → graceful shutdown axum), статусы Running/Stopped отдаются корректно;
  `install` фиксирует настройки в аргументах binPath (авто-старт, LocalSystem).

## HTTP API

| Метод | Путь | Описание |
|---|---|---|
| `POST` | `/convert?image-dpi=300` | Тело запроса — байты .docx; ответ — `application/pdf` |
| `GET` | `/health` | Liveness-проба, отвечает `ok` |

Параметр `image-dpi` (допускается и `image_dpi`): целевое разрешение растровых
картинок в PDF. Диапазон 1–2400, по умолчанию 220 (как в Word и CLI dxpdf).

Коды ответов:

- `200` — PDF в теле ответа (`Content-Disposition: attachment`);
- `400` — некорректный `image-dpi` или пустое тело;
- `413` — тело больше лимита (`--max-body-mb`, по умолчанию 100 МБ);
- `422` — файл не парсится как DOCX (текст ошибки dxpdf в теле);
- `500` — паника конвертера (в лог пишется причина).

Пример:

```bash
curl --data-binary @document.docx "http://192.168.1.33:8080/convert?image-dpi=300" -o document.pdf
```

Готовые сборки — на странице
[Releases](../../releases) (архив с exe публикуется GitHub Actions по тегу `v*`).

## Сборка (на Windows)

Нужны: rustup (MSVC toolchain) и VS Build Tools. `skia-safe` скачивает готовые
бинарники Skia — clang/python не требуются.

```powershell
cd dxpdf-service
cargo build --release   # рядом должна лежать папка ../dxpdf
```

## Установка / эксплуатация

```powershell
# скопировать exe в стабильное место (не из target\ — rebuild залочит файл)
Copy-Item target\release\dxpdf-service.exe C:\svc\bin\

# установка + запуск (авто-старт при загрузке ОС, LocalSystem)
C:\svc\bin\dxpdf-service.exe install --port 8080 --log-file C:\svc\dxpdf-service.log

# управление
Restart-Service DxPdfService
Stop-Service DxPdfService
C:\svc\bin\dxpdf-service.exe uninstall

# отладка в консоли (без SCM, Ctrl+C останавливает)
dxpdf-service.exe run --port 8091
```

Флаги `install`/`run`/`service`: `--host` (0.0.0.0), `--port` (8080),
`--max-body-mb` (100), `--concurrency` (0 = число CPU), `--log-file`
(в режиме службы по умолчанию — `dxpdf-service.log` рядом с exe).
Уровень логов — переменная `RUST_LOG` (по умолчанию `info`).

Если служба сразу останавливается с Event ID 7024 «Incorrect function»
(service-specific error 1) — HTTP-сервер не смог стартовать; почти всегда это
занятый порт. Смотрите причину в лог-файле, занявший порт процесс — через
`netstat -ano | findstr :8080`; либо переустановите службу на другой порт.

Для доступа извне нужно входящее правило файрвола:

```powershell
New-NetFirewallRule -DisplayName 'DxPdfService HTTP 8080' -Direction Inbound -Protocol TCP -LocalPort 8080 -Action Allow
```

## Обновление

```powershell
Stop-Service DxPdfService
cargo build --release
Copy-Item target\release\dxpdf-service.exe C:\svc\bin\ -Force
Start-Service DxPdfService
```
