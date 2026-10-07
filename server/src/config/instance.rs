//! Instance-wide settings of the keestore module, as the administrator left them
//! in the console.
//!
//! Declared by `module.toml`'s `[[settings]]`, stored in `core.settings`, and read
//! back here through `/internal/modules/keestore/settings` — a module owns its own
//! schema and cannot read the core's tables, and a background worker has no user
//! token for the public config route. The module is named in the URL so the read
//! works whether the instance shares one master secret or a derived one per
//! module.
//!
//! Keestore is ZERO-KNOWLEDGE: the vault (`.kdbx`) is encrypted on the client and
//! the server never sees the master password or the entries. Nearly everything a
//! password manager might expose to an admin (generation, complexity policy,
//! lockout, attempts, sharing) is therefore impossible to enforce server-side and
//! is deliberately absent here. Only two knobs act on server-visible behaviour.
//!
//! Every field here is read by code that acts on it: a knob that changes nothing
//! is worse than an absent one.

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct InstanceConfig {
    /// Ceiling, in bytes, on an uploaded `.kdbx` vault. A larger `PUT /kdbx` is
    /// rejected with `FileTooLarge`.
    pub max_kdbx_size_bytes: u64,
    /// Whether the breach-check proxy is available. When `false`, the
    /// `GET /hibp/:prefix` route short-circuits and the feature is off for every
    /// user of this instance.
    pub enable_hibp: bool,
    /// Range endpoint of the k-anonymity breach-check service. Lets an instance
    /// point at a self-hosted mirror instead of the public service, so that not
    /// even a 5-character hash prefix leaves the network. Empty keeps the value
    /// configured in `config.toml`.
    pub hibp_api_url: String,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            // 50 MB — matches `keestore.max_kdbx_size_bytes` default in settings.rs.
            max_kdbx_size_bytes: 52_428_800,
            enable_hibp:         true,
            // Same default as `hibp.api_url` in settings.rs.
            hibp_api_url:        "https://api.pwnedpasswords.com/range".to_string(),
        }
    }
}

impl InstanceConfig {
    /// Maps the core's `{key: value}` object onto the struct. Every read falls
    /// back to the compiled default rather than to a permissive value; a
    /// non-positive or out-of-range size is treated as a mistake and ignored.
    pub fn from_settings(settings: &Value) -> Self {
        let d = Self::default();
        let size = settings
            .get("max_kdbx_size_bytes")
            .and_then(Value::as_i64)
            .filter(|n| *n > 0)
            .map(|n| n as u64)
            .unwrap_or(d.max_kdbx_size_bytes);
        let enable_hibp = settings
            .get("enable_hibp")
            .and_then(Value::as_bool)
            .unwrap_or(d.enable_hibp);
        // A blank field must not disable breach checking by pointing it nowhere,
        // so an empty value keeps the compiled default.
        let hibp_api_url = settings
            .get("hibp_api_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or(d.hibp_api_url);

        Self {
            max_kdbx_size_bytes: size,
            enable_hibp,
            hibp_api_url,
        }
    }
}

/// Reads the instance settings from the core. Any failure yields `None`, so the
/// caller keeps the values it already had rather than reverting to defaults
/// because the core was briefly unreachable.
pub async fn fetch(http: &reqwest::Client, core_url: &str, secret: &str) -> Option<InstanceConfig> {
    let url = format!("{core_url}/internal/modules/keestore/settings");
    let resp = http
        .get(&url)
        .header("X-Internal-Secret", secret)
        .send()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Lecture des réglages d'instance keestore"))
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), "Réglages d'instance keestore refusés par le core");
        return None;
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| tracing::warn!(error = %e, "Réglages d'instance keestore : réponse illisible"))
        .ok()?;

    Some(InstanceConfig::from_settings(body.get("settings")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_keys_keep_the_compiled_defaults() {
        let c = InstanceConfig::from_settings(&json!({}));
        assert_eq!(c.max_kdbx_size_bytes, 52_428_800);
        assert!(c.enable_hibp);
    }

    #[test]
    fn hibp_can_be_disabled() {
        let c = InstanceConfig::from_settings(&json!({ "enable_hibp": false }));
        assert!(!c.enable_hibp);
    }

    #[test]
    fn non_positive_size_falls_back() {
        let c = InstanceConfig::from_settings(&json!({ "max_kdbx_size_bytes": 0 }));
        assert_eq!(c.max_kdbx_size_bytes, 52_428_800);
    }

    #[test]
    fn breach_endpoint_is_read_and_blank_falls_back() {
        let c = InstanceConfig::from_settings(&json!({ "hibp_api_url": "https://hibp.interne.example/range" }));
        assert_eq!(c.hibp_api_url, "https://hibp.interne.example/range");

        let blank = InstanceConfig::from_settings(&json!({ "hibp_api_url": "   " }));
        assert_eq!(blank.hibp_api_url, InstanceConfig::default().hibp_api_url);
    }

    #[test]
    fn valid_size_is_read() {
        let c = InstanceConfig::from_settings(&json!({ "max_kdbx_size_bytes": 104_857_600i64 }));
        assert_eq!(c.max_kdbx_size_bytes, 104_857_600);
    }
}
