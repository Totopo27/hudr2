//! Gestión de cuotas y proveedores de telemetría de cuentas.
//!
//! Desacopla la lógica de proveedores específicos (como CLIProxyAPI)
//! del ciclo de vida general y de la interfaz de usuario.

pub mod cliproxy;

use crate::models::AccountQuota;

pub struct QuotaManager;

impl QuotaManager {
    /// Comprueba si hay un proveedor de cuotas disponible y configurado con credenciales válidas.
    pub fn is_available() -> bool {
        cliproxy::is_available()
    }

    /// Descubre cuentas disponibles delegando al backend configurado.
    pub fn discover_accounts(target_list: &mut Vec<AccountQuota>) {
        cliproxy::discover_accounts(target_list);
    }

    /// Consulta y actualiza la cuota remota para una cuenta dada.
    pub async fn fetch_quota_for_account(acc: &mut AccountQuota) {
        cliproxy::fetch_quota_for_account(acc).await;
    }

    /// Aplica datos crudos de cuota (JSON) a la estructura de la cuenta.
    pub fn apply_quota_data(acc: &mut AccountQuota, data: &serde_json::Value) {
        cliproxy::apply_quota_data(acc, data);
    }

    /// Registra estados de error HTTP sobre la estructura de la cuenta.
    pub fn apply_quota_error(acc: &mut AccountQuota, status: u16, err_val: &serde_json::Value) {
        cliproxy::apply_quota_error(acc, status, err_val);
    }
}

/// Alias / fachada para mantener compatibilidad total con llamadas existentes a `QuotaCollector`.
pub type QuotaCollector = QuotaManager;

pub use cliproxy::{calculate_quota_pace, format_relative_reset};
