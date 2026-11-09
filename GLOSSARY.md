# Glosario: hudr2

- **Herdr**: Multiplexor de terminal orientado a flujos de trabajo con agentes de IA.
- **Pane**: Panel rectangular dentro de una pestaña del multiplexor con su propio PTY e identificador (`w1:p1`, `HERDR_PANE_ID`).
- **Workspace**: Espacio de trabajo asociado a una ruta de proyecto en disco (`HERDR_WORKSPACE_ID`).
- **Tab**: Pestaña que agrupa uno o más paneles (`HERDR_TAB_ID`).
- **Session (OpenCode / Pi)**: Registro de agente activo en SQLite (`session`/`session_v2`) o `.jsonl` de Pi que almacena el modelo, consumo de tokens, herramientas y marcas de tiempo del turno.
- **Account Prefix**: Prefijo asociado a credenciales locales de Cliproxy, CPAMC o Antigravity (por ejemplo `acc-1`, `cuenta1`, `cuenta4`) guardadas en disco.
- **Quota Pool**: Cuotas de consumo con ventanas rotativas (5 horas y semanales) reportadas por el endpoint de cuotas para familias de modelos (`Gemini`, `Claude`, `GPT`, `DeepSeek`, `Qwen`).
- **Quick Peek**: Modal flotante de inspección rápida de archivos o diffs de Git dentro del HUD (`Espacio` o clic) sin abandonar la terminal.
- **Pre-warming en Zona Amarilla**: Validación preventiva en segundo plano de candidatas a migración cuando una cuenta activa entra en advertencia (<= 30% libre), para que la migración ('x') sea instantánea al llegar a nivel crítico.
- **Deep Module**: Módulo con interfaz pública reducida y coherente que concentra la lógica de sockets, serialización y acceso a datos en su interior evitando fachadas superficiales.
