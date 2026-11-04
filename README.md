# hudr2

Sidebar de telemetría y estado para Herdr, Telar y OpenCode. Corre nativo en Rust con Ratatui y Tokio.

---

## Módulos del sistema

- **`herdr` (`HerdrClient`)**: Resuelve qué pane está activo, en qué workspace y qué tab tiene el foco.
- **`opencode`**: Extrae tokens usados en el turno, modelo activo, herramientas invocadas y costos desde `opencode.db`.
- **`engram`**: Consulta recuento de observaciones y estado de sincronización local/remota desde `engram.db`.
- **`git`**: Monitorea branch, archivos pendientes (staged/untracked) y el commit más reciente de forma no bloqueante.
- **`quotas`**: Consulta de fondo con Tokio y Reqwest para el estado de cuotas en Claude, Antigravity, etc. / Cliproxy.
- **`ui`**: Dibuja las tarjetas en terminal usando Ratatui con paleta TrueColor de 24 bits.

---

## Atajos de teclado

- `1` a `9`: Plegar o desplegar tarjetas individuales (1: Estado, 2: Contexto, 3: Proyecto, 4: Integraciones, 5: Cuotas de Modelos, 6: Cambios, 7: Engram, 8: Herramientas, 9: MCP).
- `c`: Colapsar o abrir todas las tarjetas a la vez.
- `d`: Liberar sesiones inactivas y limpiar retenciones de cuentas en SQLite/Herdr.
- `x`: Migrar en caliente las sesiones que estén usando una cuenta en estado crítico (>85% gastado) hacia la cuenta sana con mejor cuota disponible.
- `s`: Sincronizar modelos e integraciones de Cliproxy en `opencode.json`.
- `r`: Forzar actualización de cuotas y estado de red.
- `o`: Abrir la carpeta del proyecto en el explorador de archivos.
- `k` / `j` (o flechas `↑` / `↓`): Mover la vista verticalmente.
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
- `ENGRAM_DB_PATH`: Ubicación exacta de `engram.db`.
- `CLIPROXY_AUTHS_DIR`: Carpeta donde residen los tokens JSON de cuentas de Cliproxy.
- `OPENCODE_CONFIG_DIR`: Carpeta que contiene `opencode.json`.
- `ENGRAM_BIN`: Ruta manual al ejecutable de `engram`.
