# Glosario: hudr2

- **Herdr / Telar**: Multiplexor de terminal orientado a flujos de trabajo con agentes de IA. Telar es una variante derivada de la base de Herdr.
- **Pane**: Panel rectangular dentro de una pestaña del multiplexor con su propio PTY e identificador (`w1:p1`, `HERDR_PANE_ID`).
- **Workspace**: Espacio de trabajo asociado a una ruta de proyecto en disco (`HERDR_WORKSPACE_ID`).
- **Tab**: Pestaña que agrupa uno o más paneles (`HERDR_TAB_ID`).
- **Session (OpenCode)**: Entrada en la tabla `session_v2` de SQLite que almacena el modelo, agente, consumo de tokens y marcas de tiempo del turno.
- **Account Prefix**: Prefijo asociado a credenciales locales de Cliproxy o Antigravity (por ejemplo `acc-1`, `nina`, `gianni`) guardadas en disco.
- **Quota Pool**: Cuotas de consumo con ventanas rotativas (5 horas y semanales) reportadas por el servicio de Cloud Code.
- **Deep Module**: Módulo con interfaz pública reducida que concentra la lógica de sockets, deserialización y consultas a base de datos en su interior.
- **Seam**: Punto de corte en la arquitectura para alternar transportes o lectores durante pruebas unitarias (por ejemplo `HerdrTransport`).
