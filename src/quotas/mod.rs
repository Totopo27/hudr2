//! Gestión de cuotas y proveedores de telemetría de cuentas.
//!
//! Desacopla la lógica de proveedores específicos (como CLIProxyAPI)
//! del ciclo de vida general y de la interfaz de usuario.

pub mod cliproxy;

pub use cliproxy::*;
