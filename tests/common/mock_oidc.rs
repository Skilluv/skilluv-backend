//! Minimal mock OIDC IdP for enterprise SSO integration tests.
//!
//! Implements the subset of OpenID Connect required by the `openidconnect`
//! crate: discovery, JWKS, authorize, token. Signs ID tokens with RS256 using
//! a fresh RSA-2048 keypair generated at spawn time.
//!
//! The mock is stateful about pending authorizations so it can enforce PKCE
//! (S256) and echo the correct `nonce` in issued ID tokens.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Form, Query, State};
use axum::http::HeaderMap;
use axum::response::Redirect;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{EncodingKey, Header, encode as jwt_encode};
use rsa::pkcs8::EncodePrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct MockIdp {
    pub base_url: String,
    pub client_id: String,
    pub client_secret: String,
    _inner: Arc<Inner>,
}

struct Inner {
    issuer: String,
    private_pem: String,
    public_n_b64: String,
    public_e_b64: String,
    client_id: String,
    client_secret: String,
    codes: Mutex<HashMap<String, PendingAuth>>,
    subject_email: String,
    subject_name: String,
}

#[derive(Clone, Debug)]
struct PendingAuth {
    nonce: String,
    redirect_uri: String,
    code_challenge: String,
}

impl MockIdp {
    /// Spawn a mock IdP on a random localhost port. The IdP will issue an
    /// ID token whose `email` and `name` claims match the given values, so
    /// callers can assert JIT provisioning downstream.
    pub async fn spawn(subject_email: &str, subject_name: &str) -> Self {
        // RSA 2048 key generation is ~500 ms — acceptable for a per-test cost.
        let mut rng = rand::thread_rng();
        let private_key =
            RsaPrivateKey::new(&mut rng, 2048).expect("RSA keypair generation failed");
        let public_key = RsaPublicKey::from(&private_key);

        let private_pem = private_key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .expect("pkcs8 encode")
            .to_string();

        let n_bytes = public_key.n().to_bytes_be();
        let e_bytes = public_key.e().to_bytes_be();
        let public_n_b64 = URL_SAFE_NO_PAD.encode(&n_bytes);
        let public_e_b64 = URL_SAFE_NO_PAD.encode(&e_bytes);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock IdP");
        let addr = listener.local_addr().expect("addr");
        let base_url = format!("http://{addr}");

        let client_id = format!("mock-client-{}", Uuid::new_v4().simple());
        let client_secret = format!("mock-secret-{}", Uuid::new_v4().simple());

        let inner = Arc::new(Inner {
            issuer: base_url.clone(),
            private_pem,
            public_n_b64,
            public_e_b64,
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            codes: Mutex::new(HashMap::new()),
            subject_email: subject_email.to_string(),
            subject_name: subject_name.to_string(),
        });

        let router = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/jwks", get(jwks))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .with_state(inner.clone());

        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("mock IdP serve");
        });

        Self {
            base_url,
            client_id,
            client_secret,
            _inner: inner,
        }
    }
}

// ─── Handlers ─────────────────────────────────────────────────────

async fn discovery(State(inner): State<Arc<Inner>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "issuer": inner.issuer,
        "authorization_endpoint": format!("{}/authorize", inner.issuer),
        "token_endpoint": format!("{}/token", inner.issuer),
        "jwks_uri": format!("{}/jwks", inner.issuer),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": ["openid", "email", "profile"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic", "client_secret_post"],
        "claims_supported": ["sub", "iss", "aud", "exp", "iat", "nonce", "email", "email_verified", "name"],
        "code_challenge_methods_supported": ["S256"],
    }))
}

async fn jwks(State(inner): State<Arc<Inner>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "keys": [{
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": "mock-key-1",
            "n": inner.public_n_b64,
            "e": inner.public_e_b64,
        }]
    }))
}

#[derive(Deserialize)]
struct AuthorizeQuery {
    client_id: String,
    redirect_uri: String,
    state: String,
    nonce: String,
    code_challenge: String,
    code_challenge_method: String,
    response_type: String,
    #[allow(dead_code)]
    scope: Option<String>,
}

async fn authorize(
    State(inner): State<Arc<Inner>>,
    Query(q): Query<AuthorizeQuery>,
) -> Result<Redirect, (axum::http::StatusCode, String)> {
    if q.response_type != "code" {
        return Err((axum::http::StatusCode::BAD_REQUEST, "unsupported response_type".into()));
    }
    if q.code_challenge_method != "S256" {
        return Err((axum::http::StatusCode::BAD_REQUEST, "S256 required".into()));
    }
    if q.client_id != inner.client_id {
        return Err((axum::http::StatusCode::UNAUTHORIZED, "bad client_id".into()));
    }

    let code = format!("code-{}", Uuid::new_v4().simple());
    inner.codes.lock().await.insert(
        code.clone(),
        PendingAuth {
            nonce: q.nonce,
            redirect_uri: q.redirect_uri.clone(),
            code_challenge: q.code_challenge,
        },
    );

    let sep = if q.redirect_uri.contains('?') { '&' } else { '?' };
    let target = format!("{}{sep}code={code}&state={}", q.redirect_uri, q.state);
    Ok(Redirect::to(&target))
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: String,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
    code_verifier: String,
}

#[derive(Serialize)]
struct IdTokenClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: i64,
    iat: i64,
    nonce: String,
    email: String,
    email_verified: bool,
    name: String,
}

async fn token(
    State(inner): State<Arc<Inner>>,
    headers: HeaderMap,
    Form(form): Form<TokenForm>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if form.grant_type != "authorization_code" {
        return Err((axum::http::StatusCode::BAD_REQUEST, "grant_type".into()));
    }

    // Accept both `client_secret_basic` (Authorization: Basic base64(id:secret))
    // and `client_secret_post` (id/secret in form body). openidconnect picks
    // basic when the server declares it as supported.
    let (cid, csec) = if let Some(auth) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Basic "))
    {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(auth)
            .map_err(|_| (axum::http::StatusCode::UNAUTHORIZED, "bad basic".into()))?;
        let s = String::from_utf8(decoded)
            .map_err(|_| (axum::http::StatusCode::UNAUTHORIZED, "bad basic utf8".into()))?;
        let (id, sec) = s
            .split_once(':')
            .ok_or((axum::http::StatusCode::UNAUTHORIZED, "no colon".into()))?;
        // URL-decode: openidconnect percent-encodes client_id/secret in Basic auth (RFC 6749 §2.3.1).
        let id = urldecode(id);
        let sec = urldecode(sec);
        (id, sec)
    } else {
        (
            form.client_id.clone().unwrap_or_default(),
            form.client_secret.clone().unwrap_or_default(),
        )
    };

    if cid != inner.client_id || csec != inner.client_secret {
        return Err((axum::http::StatusCode::UNAUTHORIZED, "bad client creds".into()));
    }

    let pending = {
        let mut codes = inner.codes.lock().await;
        codes.remove(&form.code)
    }
    .ok_or((axum::http::StatusCode::BAD_REQUEST, "unknown code".into()))?;

    // PKCE S256 verification: SHA256(verifier), base64url no-pad, must equal challenge.
    let mut hasher = Sha256::new();
    hasher.update(form.code_verifier.as_bytes());
    let computed = URL_SAFE_NO_PAD.encode(hasher.finalize());
    if computed != pending.code_challenge {
        return Err((axum::http::StatusCode::UNAUTHORIZED, "PKCE mismatch".into()));
    }

    let now = chrono::Utc::now().timestamp();
    let claims = IdTokenClaims {
        iss: inner.issuer.clone(),
        sub: format!("mock-sub-{}", inner.subject_email),
        aud: inner.client_id.clone(),
        exp: now + 300,
        iat: now,
        nonce: pending.nonce,
        email: inner.subject_email.clone(),
        email_verified: true,
        name: inner.subject_name.clone(),
    };

    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("mock-key-1".to_string());

    let id_token = jwt_encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(inner.private_pem.as_bytes())
            .expect("rsa pem for signing"),
    )
    .expect("sign id_token");

    Ok(Json(serde_json::json!({
        "access_token": format!("access-{}", Uuid::new_v4().simple()),
        "token_type": "Bearer",
        "expires_in": 300,
        "scope": "openid email profile",
        "id_token": id_token,
    })))
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}
