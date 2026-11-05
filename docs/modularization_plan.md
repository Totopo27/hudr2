# Informe de Arquitectura y Modularización para `hudr2`

**Fecha:** 5 de noviembre de 2026  
**Objetivo:** Evaluar la viabilidad, impacto y estrategia técnica para transformar `hudr2` de un monitor acoplado a OpenCode y CLIProxyAPI (CPAMC) hacia una plataforma extensible multimodelo y multiagente (OpenCode, Pi, Gentle Shell, etc.) con gestión desacoplada de cuotas.

---

## 1. Contexto y Diagnóstico Actual

Actualmente, `hudr2` funciona como un dashboard/TUI en tiempo real para WezTerm con un flujo reactivo:
1. **Detección de entorno:** Detecta panes activos en WezTerm (`herdr_panes`).
2. **Telemetría de sesión:** Lee la base de datos SQLite de OpenCode (`opencode.db` en modo solo lectura).
3. **Gestión de cuotas y cuentas:** Lee credenciales JSON en carpetas de `cliproxyapi`/`auths` y consulta un endpoint REST (`QUOTA_URL`) servido por un plugin de CLIProxyAPI.
4. **Mantenimiento y automatización:** El subsistema de `cleanup.rs` cambia modelos degradados asumiendo perfiles de CLIProxy (`cliproxy/{profile}/{model}`), y `sync.rs` inyecta modelos en `opencode.json` bajo la clave `"cliproxy"`.

Aunque el refactor reciente (`6f994bf refactor: split opencode.rs into opencode/ submodules`) dividió limpiamente la lógica de OpenCode en `model_info`, `prefix` y `session`, el sistema mantiene dos acoplamientos fundamentales:
* **Acoplamiento de Agente:** La aplicación asume que el runtime ejecutándose en el terminal es OpenCode.
* **Acoplamiento de Proveedor de Cuotas:** Asume que los modelos y credenciales provienen obligatoriamente del ecosistema CLIProxyAPI.

---

## 2. Eje 1: Modularización Multiagente (OpenCode, Pi, Gentle Shell)

### 2.1. Análisis de Factibilidad

Soportar agentes alternativos como **Pi (`pi.dev`)** o **Gentle Shell** es totalmente viable, pero requiere entender que cada agente almacena y expone su telemetría mediante mecanismos distintos:

| Agente | Mecanismo de Telemetría | Reto de Integración |
| :--- | :--- | :--- |
| **OpenCode** | SQLite centralizado (`opencode.db`, tablas `session_v2`, `message`, etc.). | Ya resuelto. Consultas SQL concurrentes en solo lectura. |
| **Pi (`pi.dev`)** | CLI interactivo; persiste sesiones en estructuras locales (archivos JSON/SQLite por workspace o `~/.pi/sessions/`). | Requiere descubrir la ruta del workspace activo e indexar los JSONs de logs/sesión sin bloquear el proceso de Pi. |
| **Gentle Shell** | Multiplexor/Orquestador de workflows (vinculado a SDD / Gentle AI); expone estados vía `.gentle/`, pipes o archivos de estado temporal. | Parseo de archivos de estado ligero o lectura de descriptores de terminal. |

### 2.2. Propuesta de Abstracción: `AgentSessionProvider`

Para desacoplar la UI de OpenCode, se debe transformar el modelo de datos `OpenCodeSession` en una abstracción genérica:

```rust
// models.rs (o agent/mod.rs)
pub struct AgentSession {
    pub provider_name: String,      // "opencode", "pi", "gentle-shell"
    pub agent_name: String,         // "sdd-orchestrator", "coder", etc.
    pub model: String,
    pub model_short: String,
    pub context_limit: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub agent_status: String,       // "idle", "busy", "waiting"
    pub current_tool: Option<String>,
    pub subagents: Vec<SubagentInfo>,
}

pub trait AgentTelemetryProvider: Send + Sync {
    fn detect(&self, pane: &HerdrPane, target_dir: &str) -> bool;
    fn read_session(&self, pane: &HerdrPane, target_dir: &str) -> Option<AgentSession>;
}
```

### 2.3. Estrategia de Detección de Agentes
Para saber qué proveedor consultar por cada pane de WezTerm:
1. **Inspección de procesos:** Revisar el árbol de procesos en ejecución del pane (`opencode`, `pi`, `gentle-shell`).
2. **Inspección de archivos de workspace:** Detectar si el directorio actual contiene carpetas `.opencode/`, `.pi/` o `.gentle/`.
3. **Configuración manual (fallback):** Permitir declarar en `config.toml` el agente por defecto o por ruta.

---

## 3. Eje 2: Modularización de Cuotas y Cuentas (CLIProxyAPI vs. Otros)

### 3.1. Diagnóstico del Estado Actual

En el código actual:
* `src/paths.rs` busca credenciales en rutas fijas de CLIProxy: `~/.cliproxy/auths`, `C:/cliproxyapi/auths`, etc.
* `src/quotas.rs` asume el esquema de nombres `antigravity-*.json`, `google-*.json` y llama a la API de cuotas del plugin de CPAMC.
* `src/opencode/prefix.rs` y `src/cleanup.rs` asumen la existencia de prefijos `cliproxy/` para limpiar nombres de modelos o alternar cuentas degradadas.
* `src/sync.rs` modifica exclusivamente la sección `"cliproxy"` dentro de `opencode.json`.

Si un usuario no utiliza CLIProxyAPI (usa API keys directas, OAuth nativo de OpenCode, LiteLLM o modelos locales con Ollama), `hudr2` experimenta fallos silenciosos, warnings y paneles vacíos.

### 3.2. Propuesta de Abstracción: `QuotaSource`

El sistema de cuotas debe pasar a ser un componente opcional y desacoplado mediante una interfaz:

```rust
pub enum QuotaBackend {
    None,            // API keys directas o local (sin panel de cuotas en TUI)
    CliProxyApi,     // Backend actual (CPAMC + auths dir + plugin HTTP)
    OpenRouter,      // Futuro: consulta de créditos vía API de OpenRouter
    LocalOllama,     // Futuro: estado de modelos cargados en VRAM
}

pub trait QuotaProvider: Send + Sync {
    fn discover_accounts(&self) -> Vec<AccountQuota>;
    async fn fetch_quotas(&self, accounts: &mut [AccountQuota]);
}
```

### 3.3. Adaptaciones Requeridas en la Lógica de Negocio

1. **Panel condicional en la TUI (`src/ui.rs`):**
   * Si el backend de cuotas es `None`, el layout de la TUI se adapta: en lugar de mostrar una tabla de cuentas vacía con 0%, el espacio se aprovecha para expandir el historial de cambios (`AttributedChanges`), métricas de contexto o logs del workspace.
2. **Neutralidad de nombres de modelo (`src/opencode/prefix.rs` y `cleanup.rs`):**
   * Eliminar el `.replace("cliproxy/", "")` hardcodeado.
   * Utilizar una función de normalización que reciba los prefijos conocidos del backend activo.
3. **Comportamiento del Auto-Cleanup:**
   * En modo CLIProxy: conmuta perfiles (`nina`, `antigravity-1`, etc.).
   * En modo API directa: alerta al usuario que el modelo superó el límite de contexto o falló, sin intentar manipular prefijos inexistentes.
4. **Comando `sync` modular:**
   * `hudr2 sync` debe requerir un argumento o detectar el proveedor (ej. `hudr2 sync --cliproxy` o leer de `config.toml` qué provider se desea sincronizar).

---

## 4. Matriz de Impacto en la Base de Código

| Componente Actual | Archivo(s) | Cambio Necesario para Modularizar |
| :--- | :--- | :--- |
| **Modelos de datos** | `src/models.rs` | Renombrar `OpenCodeSession` a `AgentSession`; independizar los campos de quotas haciéndolos `Option<T>`. |
| **Telemetría Agentes** | `src/opencode/` $\to$ `src/agents/` | Mover `opencode/` a `agents/opencode/` e implementar la interfaz común para `agents/pi/` y `agents/gentle_shell/`. |
| **Colectores de Cuotas** | `src/quotas.rs`, `src/paths.rs` | Encapsular la búsqueda de `auths` y el polling HTTP dentro de `src/quotas/cliproxy.rs`. Permitir modo `Disabled`/`None`. |
| **Normalización Prefijos** | `src/opencode/prefix.rs` | Generalizar el parseo de modelos (`vendor/model` o `profile/model`) sin asumir `cliproxy/`. |
| **Cleanup de Sesiones** | `src/cleanup.rs` | Desacoplar la mutación de la DB de OpenCode y el cambio de modelo vía JSON para que sea una acción provista por el adaptador del agente activo. |
| **Interfaz de Usuario** | `src/ui.rs` | Diseñar un layout responsivo que oculte de forma natural el panel de cuotas cuando no haya backend configurado. |
| **Configuración** | `src/config.rs` | Añadir bloque `[agent]` y `[quotas]` en `config.toml` para definir qué adaptadores activar. |

---

## 5. Hoja de Ruta Sugerida para Futura Refactorización

Para cuando decidas llevar a cabo esta transición sin romper lo que hoy funciona de forma estable:

### Fase 1: Desacoplamiento de Cuotas (Menor riesgo funcional)
1. Convertir `QuotaCollector` en un módulo opcional.
2. Hacer que si no existe la carpeta `auths` o `QUOTA_URL` no responde, `hudr2` oculte limpiamente el widget de cuotas en la TUI sin emitir errores de ejecución.
3. Extraer las rutas específicas de CLIProxy a su propio submódulo (`src/quotas/cliproxy.rs`).

### Fase 2: Generalización de Modelos de Sesión
1. Renombrar internamente `OpenCodeSession` a un modelo genérico de sesión.
2. Asegurar que las vistas en `ui.rs` solo consuman el modelo genérico, sin conocer detalles internos de SQLite.

### Fase 3: Introducción del Trait de Agentes
1. Crear el trait `AgentTelemetryProvider`.
2. Convertir el actual `src/opencode/` en la primera implementación del trait (`OpenCodeProvider`).
3. Crear un mock o adaptador inicial para Pi o Gentle Shell implementando la lectura de su formato específico de persistencia.
