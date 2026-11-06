# hudr2

Sidebar de telemetría y estado reactivo para Herdr/Telar, **OpenCode** y **Pi / Gentle Shell**. Corre nativo en Rust con arquitectura impulsada por eventos (0% CPU en reposo).

---

## Módulos del sistema

- **`herdr` (`HerdrClient`)**: Conexión directa por socket/named-pipe NDJSON (`events.subscribe`) para enterarse en tiempo real de cambios de pane, estado del agente y cierre de pestaña (con CLI `herdr` como respaldo).
- **`opencode`**: Extrae tokens usados en el turno, modelo activo, herramientas invocadas y costos desde `opencode.db` (solo lectura).
- **`pi`**: Lector incremental por byte-offset (`PiTail`) para sesiones `.jsonl` de **Pi / Gentle Shell (`gs2`)**, con resolución dinámica de ventana de contexto (`cpamc-models-cache.json` / `models-store.json`) y fallback a `settings.json`.
- **`engram`**: Consulta observaciones, estado del daemon local (`127.0.0.1:7437/health`), sincronización y el último resumen de sesión (**Active Handoff** `session_summary`) desde `engram.db`.
- **`git`**: Monitorea rama, ahead/behind, líneas `+/-` (`--numstat`) y archivos modificados con `GIT_OPTIONAL_LOCKS=0`, timeout de 3s y **previsualización de diff inline** con filtro anti-secretos (`.env`, claves, tokens).
- **`ecosystem`**: Sondeo *one-shot* con timeout de las versiones instaladas del stack (`pi`, `hudr2`, `gentle-shell`, `gentle-ai`, `engram`, `herdr`).
- **`quotas`**: Consulta de fondo con Tokio y Reqwest para el estado de cuotas en Claude, Antigravity, etc. / Cliproxy.
- **`ui` / `state`**: Renderizado TrueColor de 24 bits bajo demanda (redibuja únicamente cuando `TelemetrySnapshot` cambia), captura de ratón *button-only* SGR y persistencia atómica de tarjetas colapsadas (`hudr2.json`).

---

## Opciones de línea de comandos

```bash
hudr2 [--once] [--pane <HERDR_PANE_ID>] [--cwd <dir>] [--config <file>]
```

- `--once`: Imprime una captura instantánea de todas las tarjetas en consola y termina de inmediato (sin modo raw ni pantalla alternativa).
- `--pane <id>`: Enlaza el HUD a un pane específico de Herdr y se cierra automáticamente cuando dicho pane se cierra.
- `--cwd <dir>`: Directorio del proyecto a monitorear.
- `--config <file>`: Ruta personalizada para guardar el estado de tarjetas colapsadas.

---

## Atajos de teclado y ratón

- `1` a `9`: Plegar o desplegar tarjetas individuales (1: Estado, 2: Contexto, 3: Proyecto, 4: Integraciones, 5: Cuotas de Modelos, 6: Cambios, 7: Engram, 8: Herramientas, 9: MCP).
- `0` o `e`: Plegar o desplegar la tarjeta de Stack · Ecosistema.
- `c`: Colapsar o abrir todas las tarjetas a la vez (se recuerda automáticamente entre sesiones).
- **Clic en un archivo de la tarjeta `Cambios`**: Despliega o cierra su vista previa de `git diff` inline (bloqueando automáticamente archivos sensibles como `.env` o claves privadas).
- `d`: Liberar sesiones inactivas y limpiar retenciones de cuentas en SQLite/Herdr.
- `x`: Migrar en caliente las sesiones que estén usando una cuenta en estado crítico (>85% gastado) hacia la cuenta sana con mejor cuota disponible.
- `s`: Sincronizar modelos e integraciones de Cliproxy en `opencode.json`.
- `r`: Forzar actualización completa de telemetría, versiones y cuotas de red.
- `o`: Abrir la carpeta del proyecto en el explorador de archivos.
- `k` / `j` (o flechas `↑` / `↓` / rueda del ratón): Mover la vista verticalmente.
- `q` o `Ctrl+C`: Cerrar.

---

## Uso

### Compilar
```bash
cargo build --release
```

### Ejecutar en Linux / macOS
```bash
# Lanzar directo:
./hudr2.sh

# O abrirlo acoplado en un split lateral derecho dentro de Herdr:
chmod +x attach-hudr2.sh hudr2.sh
./attach-hudr2.sh
```

### Ejecutar en Windows
```cmd
:: Lanzar directo:
hudr2.cmd

:: O acoplarlo en split lateral derecho dentro de Herdr:
attach-hudr2.cmd
```

---

## Variables de entorno

Si tus bases de datos o rutas de configuración están fuera de los directorios estándar del sistema, puedes fijarlas con:

- `OPENCODE_DB_PATH`: Ubicación exacta de `opencode.db`.
- `PI_SESSION_PATH`: Ruta explícita a un archivo `.jsonl` de sesión de Pi / Gentle Shell.
- `PI_AGENT_DIR`: Carpeta raíz del agente Pi / Gentle Shell.
- `ENGRAM_DB_PATH`: Ubicación exacta de `engram.db`.
- `ENGRAM_PORT`: Puerto HTTP local del daemon de Engram (por defecto `7437`).
- `HUDR2_CONFIG_PATH`: Ruta personalizada para el archivo de persistencia `hudr2.json`.
- `CLIPROXY_AUTHS_DIR`: Carpeta donde residen los tokens JSON de cuentas de Cliproxy.
- `OPENCODE_CONFIG_DIR`: Carpeta que contiene `opencode.json`.
- `ENGRAM_BIN`: Ruta manual al ejecutable de `engram`.
