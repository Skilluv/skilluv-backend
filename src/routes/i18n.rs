//! i18n discovery endpoint (Phase 1.13).

use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::AppState;

pub fn i18n_routes() -> Router<AppState> {
    Router::new().route("/i18n/locales", get(list_locales))
}

async fn list_locales() -> Json<Value> {
    Json(json!({
        "data": {
            "default": "en",
            "available": [
                { "code": "en", "name": "English", "direction": "ltr" },
                { "code": "fr", "name": "Français", "direction": "ltr" },
                { "code": "ar", "name": "العربية", "direction": "rtl" },
            ]
        },
        "meta": {
            "request_id": uuid::Uuid::new_v4().to_string(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }
    }))
}

/// Resolve the desired locale for a request, in order of precedence:
/// 1. Explicit `Accept-Language` header (first supported tag)
/// 2. Fallback to "en"
///
/// Per-user override (`users.preferred_language`) should be checked first when available,
/// but that requires a DB hit; do it in handlers that already load the user.
pub fn resolve_from_accept_language(header: Option<&str>) -> String {
    let Some(header) = header else {
        return "en".to_string();
    };
    let supported = ["fr", "en", "ar"];
    for tag in header.split(',') {
        let lang = tag
            .split([';', '-'])
            .next()
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if supported.contains(&lang.as_str()) {
            return lang;
        }
    }
    "en".to_string()
}

/// Default locale hint for a country (used when the user hasn't set a preference).
/// Maghreb → AR ; francophone → FR ; else EN.
#[allow(dead_code)] // reserved for the geo-onboarding auto-locale step
pub fn default_locale_for_country(iso2: Option<&str>) -> &'static str {
    match iso2.map(str::to_uppercase).as_deref() {
        Some("MA" | "TN" | "DZ" | "EG") => "ar",
        Some(
            "FR" | "BE" | "CH" | "LU" | "SN" | "CI" | "BJ" | "TG" | "ML" | "BF" | "NE" | "CM"
            | "GA" | "CG" | "TD" | "CF",
        ) => "fr",
        _ => "en",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_when_no_header() {
        assert_eq!(resolve_from_accept_language(None), "en");
    }

    #[test]
    fn picks_fr_from_fr_fr() {
        assert_eq!(
            resolve_from_accept_language(Some("fr-FR,fr;q=0.9,en;q=0.8")),
            "fr"
        );
    }

    #[test]
    fn picks_en_when_unsupported_first() {
        assert_eq!(resolve_from_accept_language(Some("zh-CN,en;q=0.5")), "en");
    }

    #[test]
    fn falls_back_for_fully_unsupported() {
        assert_eq!(resolve_from_accept_language(Some("zh,ja")), "en");
    }
}
