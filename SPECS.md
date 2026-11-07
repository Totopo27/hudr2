# Especificaciones: hudr2

## 1. Requisitos de rendimiento y memoria

El objetivo central es sustituir la versión previa en Node.js por un binario autónomo en Rust que resuelva los cuellos de botella de la sesión diaria:
- Mantener el consumo de memoria dentro del umbral de 5 a 15 MB de RAM frente a los 120-250 MB que retenía Node con V8.
- Eliminar pausas por recolección de basura y saltos de latencia al refrescar la pantalla con un bucle reactivo basado en canales `mpsc` y 0% de uso de CPU en reposo.
- Acceder a los datos de `opencode.db` y `engram.db` abriendo conexiones SQLite directas con bandera de solo lectura (`SQLITE_OPEN_READONLY`), resolución de mtime en `-wal`/`-shm` y sin bloqueos de escritura con los procesos principales.
- Conectar con Herdr y Telar mediante el canal IPC nativo (`\\.\pipe\herdr-...` en Windows o domain socket en Unix) para consultar el pane y tab activo, usando el CLI únicamente como alternativa de rescate.
- Manejo atómico y seguro de archivos de caché (`.{pid}.tmp`) para evitar condiciones de carrera o truncados corruptos.
- Cero dependencias innecesarias (sin crate `regex`, parsing de cadenas ASCII puro con helpers estándar de `str`).

## 2. Organización modular

- **`herdr` (`HerdrClient`)**: Expone una interfaz acotada para abstraer el transporte IPC y entregar snapshots del estado de panes y workspaces con cacheo de resolución de binario.
- **`opencode`**: Lector de SQLite para sesiones activas, soporte de esquemas duales (OpenCode moderno 1.19+ Drizzle y legacy `session_v2`), tokens consumidos, historial de herramientas y atribución de cuentas.
- **`pi`**: Telemetría incremental de streaming (`.jsonl`) para Pi y Gentle Shell con soporte de cambios de modelo y thinking levels.
- **`engram`**: Lector de SQLite para observaciones, estado del daemon y último handoff de sesión estructurado.
- **`git`**: Consulta ligera del árbol git con `GIT_OPTIONAL_LOCKS=0`, lectura directa de HEAD sin subprocesos, radar de colisiones multisesión y protección estricta contra traversal (`..`) y archivos sensibles.
- **`quotas`**: Tareas asíncronas con Tokio y `reqwest` para sondear cuotas contra las APIs de Cliproxy / CPAMC con freno por rate-limiting (HTTP 429), TTL de errores y precalentamiento en zona amarilla.
- **`ui`**: Renderizador reactivo ANSI en terminal con truncado UTF-8 seguro por ancho visual (`safe_truncate_pad`), soporte de 11 tarjetas interactivas y modal flotante de Quick Peek.
- **`cleanup`**: Rutinas de mantenimiento (archivado limpio de sesiones inactivas con tecla `d` y migración en caliente ante cuotas críticas con tecla `x`).
