# Especificaciones: hudr2

## 1. Requisitos de rendimiento y memoria

El objetivo central es sustituir la versión previa en Node.js por un binario autónomo en Rust que resuelva los cuellos de botella de la sesión diaria:
- Mantener el consumo de memoria dentro del umbral de 5 a 15 MB de RAM frente a los 120-250 MB que retenía Node con V8.
- Eliminar pausas por recolección de basura y saltos de latencia al refrescar la pantalla.
- Acceder a los datos de `opencode.db` y `engram.db` abriendo conexiones SQLite directas con bandera de solo lectura (`SQLITE_OPEN_READONLY`), evitando bloqueos de escritura con los procesos principales.
- Conectar con Herdr y Telar mediante el canal IPC nativo (`\\.\pipe\herdr-...` en Windows o domain socket en Unix) para consultar el pane y tab activo, usando el CLI únicamente como alternativa de rescate.

## 2. Organización modular

- **`herdr` (`HerdrClient`)**: Expone una interfaz acotada para abstraer el transporte IPC y entregar snapshots del estado de panes y workspaces.
- **`opencode`**: Lector de SQLite para sesiones activas, tokens consumidos, historial reciente de herramientas y cuentas en uso.
- **`engram`**: Lector de SQLite para observaciones y sincronización de memoria.
- **`git`**: Consulta ligera del árbol git (rama actual, archivos modificados y log corto) sin detener el bucle de eventos de la interfaz.
- **`quotas`**: Tareas asíncronas con `reqwest` y Tokio para sondear cuotas contra las APIs de Cloud Code / Cliproxy sin congelar la UI.
- **`ui`**: Bucle de dibujo a 60 FPS con Ratatui y Crossterm, con cálculo de límites por tarjeta y soporte para redimensionamiento de ventana.
- **`actions`**: Rutinas de mantenimiento (limpieza de sesiones inactivas y alternancia de tarjetas).
