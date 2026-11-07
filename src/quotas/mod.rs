//! Gestión de cuotas y proveedores de telemetría de cuentas.
//!
//! Desacopla la lógica de proveedores específicos (como CLIProxyAPI)
//! del ciclo de vida general y de la interfaz de usuario.

pub mod cliproxy;

use crate::models::AccountQuota;
use std::path::{Path, PathBuf};

pub struct QuotaManager;

impl QuotaManager {
    /// Comprueba si hay un proveedor de cuotas disponible y configurado con credenciales válidas.
    pub fn is_available() -> bool {
        cliproxy::is_available()
    }

    /// Comprueba si hay credenciales válidas considerando `target_dir` (`--cwd`).
    pub fn is_available_for(target_dir: &str) -> bool {
        cliproxy::is_available_for(target_dir)
    }

    /// Descubre cuentas disponibles delegando al backend configurado.
    pub fn discover_accounts(target_list: &mut Vec<AccountQuota>) {
        cliproxy::discover_accounts(target_list);
    }

    /// Descubre cuentas disponibles considerando `target_dir` (`--cwd`).
    pub fn discover_accounts_for(target_dir: &str, target_list: &mut Vec<AccountQuota>) {
        cliproxy::discover_accounts_for(target_dir, target_list);
    }

    /// Hidrata cuentas desde la caché compartida (`quotas_cache.json`) recalculando tiempos relativos.
    pub fn hydrate_accounts_from_cache(accounts: &mut [AccountQuota], cache_path: Option<&Path>) {
        cliproxy::hydrate_accounts_from_cache(accounts, cache_path);
    }

    /// Devuelve la ruta de `quotas_cache.json` hermana al archivo de configuración.
    pub fn quota_cache_path_for(config_path: &Path) -> PathBuf {
        cliproxy::quota_cache_path_for(config_path)
    }

    /// Consulta y actualiza la cuota remota para una cuenta dada.
    pub async fn fetch_quota_for_account(acc: &mut AccountQuota) {
        cliproxy::fetch_quota_for_account(acc).await;
    }

    /// Consulta la cuota remota y persiste el resultado en la caché compartida opcional.
    pub async fn fetch_quota_for_account_with_cache(
        acc: &mut AccountQuota,
        cache_path: Option<&Path>,
    ) -> u16 {
        cliproxy::fetch_quota_for_account_with_cache(acc, cache_path).await
    }

    /// Aplica datos crudos de cuota (JSON) a la estructura de la cuenta.
    pub fn apply_quota_data(acc: &mut AccountQuota, data: &serde_json::Value) {
        cliproxy::apply_quota_data(acc, data);
    }

    /// Registra estados de error HTTP sobre la estructura de la cuenta.
    pub fn apply_quota_error(acc: &mut AccountQuota, status: u16, err_val: &serde_json::Value) {
        cliproxy::apply_quota_error(acc, status, err_val);
    }

    /// Evalúa dinámicamente la salud de la cuenta para la familia del modelo activo en la sesión.
    pub fn evaluate_account_for_model(
        acc: &AccountQuota,
        active_model: &str,
    ) -> cliproxy::EffectiveAccountHealth {
        cliproxy::evaluate_account_for_model(acc, active_model)
    }

    /// Sincroniza los campos `rem_5h`, `rem_wk`, `is_critical`, `is_warning` de la cuenta según el modelo activo.
    pub fn sync_account_health_for_model(acc: &mut AccountQuota, active_model: &str) {
        cliproxy::sync_account_health_for_model(acc, active_model);
    }

    /// Indica si la cuenta aún no ha recibido respuesta inicial del endpoint de cuotas.
    pub fn is_unfetched(acc: &AccountQuota) -> bool {
        cliproxy::is_unfetched(acc)
    }

    /// Indica si la cuenta requiere consulta por ser nueva, haber cambiado credencial o haber vencido su `resetTime`.
    pub fn needs_refresh(acc: &AccountQuota) -> bool {
        cliproxy::needs_refresh(acc)
    }

    /// Indica si la cuenta fue verificada dentro de `max_age_secs` y sigue sana/vigente.
    pub fn is_fresh_within(acc: &AccountQuota, max_age_secs: i64) -> bool {
        cliproxy::is_fresh_within(acc, max_age_secs)
    }

    /// Indica si el freno preventivo ante HTTP 429 está activo.
    pub fn is_rate_limit_cooling_down() -> bool {
        cliproxy::is_rate_limit_cooling_down()
    }
}

/// Alias / fachada para mantener compatibilidad total con llamadas existentes a `QuotaCollector`.
pub type QuotaCollector = QuotaManager;

pub use cliproxy::{
    EffectiveAccountHealth, UNFETCHED_RESET, calculate_quota_pace, clear_quota_runtime_cache,
    discover_accounts_for, evaluate_account_for_model, format_relative_reset,
    hydrate_accounts_from_cache, is_available_for, is_fresh_within, is_rate_limit_cooling_down,
    is_unfetched, needs_refresh, quota_cache_path_for, record_cached_quota_at,
    record_cached_quota_error_at, resolve_model_family, sync_account_health_for_model,
    trigger_rate_limit_cooldown,
};
