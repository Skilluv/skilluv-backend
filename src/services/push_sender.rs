//! Web Push natif RFC 8291 (aes128gcm) + VAPID RFC 8292 — Phase 4.12.
//!
//! Implémentation pure-Rust (p256 + aes-gcm + hkdf), pas d'OpenSSL.
//! Envoie effectivement les notifications aux endpoints des browsers.
//!
//! Flow par subscription :
//!   1. Décoder les clés `p256dh` (65 bytes uncompressed EC point) et `auth` (16 bytes)
//!   2. Générer une clé éphémère P-256
//!   3. ECDH shared secret entre notre éphémère et p256dh du client
//!   4. HKDF (RFC 8291 §3.4) → key + nonce AES-128-GCM
//!   5. Encrypt payload + JSON `{title, body, url}`
//!   6. Header VAPID JWT signé ES256 + POST vers l'endpoint
//!
//! Sur 404/410, on supprime la subscription (browser désabonné).

use aes_gcm::aead::Aead;
use aes_gcm::{Aes128Gcm, KeyInit, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64_URL;
use elliptic_curve::sec1::ToEncodedPoint;
use hkdf::Hkdf;
use hmac::{KeyInit as HmacKeyInit, Mac};
use p256::PublicKey;
use p256::ecdh::EphemeralSecret;
use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use serde::Serialize;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

const AES128GCM_TAG_LEN: usize = 16;
const AES128GCM_KEY_LEN: usize = 16;
const AES128GCM_NONCE_LEN: usize = 12;

pub struct VapidConfig {
    /// Base64 URL-safe du point EC uncompressed (65 bytes → 87 chars).
    pub public_key_b64: String,
    /// Base64 URL-safe du scalar privé (32 bytes → 43 chars).
    pub private_key_b64: String,
    pub subject: String,
}

impl VapidConfig {
    pub fn from_env() -> Option<Self> {
        Some(Self {
            public_key_b64: std::env::var("VAPID_PUBLIC_KEY")
                .ok()
                .filter(|s| !s.is_empty())?,
            private_key_b64: std::env::var("VAPID_PRIVATE_KEY")
                .ok()
                .filter(|s| !s.is_empty())?,
            subject: std::env::var("VAPID_SUBJECT")
                .unwrap_or_else(|_| "mailto:ops@skilluv.com".into()),
        })
    }
}

#[derive(Serialize)]
struct PushPayload<'a> {
    title: &'a str,
    body: &'a str,
    url: &'a str,
    icon: &'a str,
}

/// Envoie une notification aux subscriptions actives d'un utilisateur.
/// Renvoie `(succeeded, failed)`.
pub async fn push_to_user(
    db: &PgPool,
    user_id: Uuid,
    title: &str,
    body: &str,
    url: Option<&str>,
) -> Result<(usize, usize), AppError> {
    let Some(vapid) = VapidConfig::from_env() else {
        tracing::debug!("VAPID not configured — skipping push");
        return Ok((0, 0));
    };
    let subs: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT id, endpoint, p256dh_key, auth_secret FROM push_subscriptions WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    if subs.is_empty() {
        return Ok((0, 0));
    }

    let payload_json = serde_json::to_vec(&PushPayload {
        title,
        body,
        url: url.unwrap_or("/"),
        icon: "/icons/icon-192.png",
    })
    .map_err(|e| AppError::Internal(format!("push payload serialize: {e}")))?;

    let client = reqwest::Client::new();
    let mut ok = 0usize;
    let mut ko = 0usize;

    for (sub_id, endpoint, p256dh, auth) in &subs {
        match send_one(&client, &vapid, endpoint, p256dh, auth, &payload_json).await {
            Ok(status) => {
                if (200..300).contains(&status) {
                    ok += 1;
                    let _ = sqlx::query(
                        "UPDATE push_subscriptions SET last_success_at = NOW(), failure_count = 0 WHERE id = $1",
                    )
                    .bind(sub_id)
                    .execute(db)
                    .await;
                } else if status == 404 || status == 410 {
                    // Browser a désabonné : purger la subscription.
                    let _ = sqlx::query("DELETE FROM push_subscriptions WHERE id = $1")
                        .bind(sub_id)
                        .execute(db)
                        .await;
                    ko += 1;
                } else {
                    ko += 1;
                    let _ = sqlx::query(
                        "UPDATE push_subscriptions SET last_failure_at = NOW(), failure_count = failure_count + 1 WHERE id = $1",
                    )
                    .bind(sub_id)
                    .execute(db)
                    .await;
                }
            }
            Err(e) => {
                ko += 1;
                tracing::warn!(sub_id = %sub_id, error = %e, "web-push send failed");
            }
        }
    }
    metrics::counter!("skilluv_push_sent_total", "status" => "ok").increment(ok as u64);
    metrics::counter!("skilluv_push_sent_total", "status" => "ko").increment(ko as u64);
    Ok((ok, ko))
}

async fn send_one(
    client: &reqwest::Client,
    vapid: &VapidConfig,
    endpoint: &str,
    p256dh_b64: &str,
    auth_b64: &str,
    payload: &[u8],
) -> Result<u16, AppError> {
    let p256dh_bytes = B64_URL
        .decode(p256dh_b64.trim_end_matches('='))
        .map_err(|e| AppError::Internal(format!("p256dh decode: {e}")))?;
    let auth_bytes = B64_URL
        .decode(auth_b64.trim_end_matches('='))
        .map_err(|e| AppError::Internal(format!("auth decode: {e}")))?;
    if p256dh_bytes.len() != 65 || auth_bytes.len() != 16 {
        return Err(AppError::Internal("invalid p256dh/auth length".into()));
    }
    let client_pub = PublicKey::from_sec1_bytes(&p256dh_bytes)
        .map_err(|e| AppError::Internal(format!("p256 pubkey decode: {e}")))?;

    // Encrypt payload (aes128gcm)
    let (encrypted_body, _) = encrypt_aes128gcm(&client_pub, &auth_bytes, payload)?;

    // VAPID JWT
    let origin = origin_from_endpoint(endpoint)
        .ok_or(AppError::Internal("cannot parse endpoint origin".into()))?;
    let jwt = build_vapid_jwt(vapid, &origin)?;

    let auth_header = format!("vapid t={jwt},k={}", vapid.public_key_b64);
    let resp = client
        .post(endpoint)
        .header("Authorization", auth_header)
        .header("Content-Encoding", "aes128gcm")
        .header("TTL", "86400")
        .header("Content-Type", "application/octet-stream")
        .body(encrypted_body)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("push send http: {e}")))?;
    Ok(resp.status().as_u16())
}

fn origin_from_endpoint(endpoint: &str) -> Option<String> {
    let after_scheme = endpoint.split_once("://")?.1;
    let host = after_scheme.split('/').next()?;
    let scheme = endpoint.split_once("://")?.0;
    Some(format!("{scheme}://{host}"))
}

/// Chiffre `payload` selon RFC 8291 (aes128gcm) + le header aes128gcm de RFC 8188.
/// Retourne (body prêt à envoyer, ephemeral_public_bytes utilisé).
fn encrypt_aes128gcm(
    client_pub: &PublicKey,
    auth_secret: &[u8],
    payload: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    // 1. Éphémère
    let ephemeral = EphemeralSecret::random(&mut rand_core::OsRng);
    let as_pub = ephemeral.public_key();
    let as_pub_bytes = as_pub.to_encoded_point(false).as_bytes().to_vec();
    if as_pub_bytes.len() != 65 {
        return Err(AppError::Internal("ephemeral pubkey size".into()));
    }

    // 2. ECDH shared secret
    let shared = ephemeral.diffie_hellman(client_pub);
    let ikm_raw = shared.raw_secret_bytes();

    // 3. HKDF pass 1 — dériver "IKM" pour la clé de content encryption
    // key_info = "WebPush: info\0" || ua_public(65) || as_public(65)
    let client_pub_bytes = client_pub.to_encoded_point(false).as_bytes().to_vec();
    let mut key_info = Vec::with_capacity(14 + 65 + 65);
    key_info.extend_from_slice(b"WebPush: info\0");
    key_info.extend_from_slice(&client_pub_bytes);
    key_info.extend_from_slice(&as_pub_bytes);

    // Salt = 16 bytes random pour ce message
    let mut salt = [0u8; 16];
    getrandom::getrandom(&mut salt)
        .map_err(|e| AppError::Internal(format!("salt getrandom: {e}")))?;

    // IKM = HKDF-Expand(HKDF-Extract(auth_secret, ecdh_shared), key_info, 32)
    let mut prk_key_ikm = [0u8; 32];
    let h1 = Hkdf::<Sha256>::new(Some(auth_secret), ikm_raw.as_slice());
    h1.expand(&key_info, &mut prk_key_ikm)
        .map_err(|_| AppError::Internal("HKDF expand ikm".into()))?;

    // 4. HKDF pass 2 — dériver CEK et nonce
    let h2 = Hkdf::<Sha256>::new(Some(&salt), &prk_key_ikm);
    let mut cek = [0u8; AES128GCM_KEY_LEN];
    h2.expand(b"Content-Encoding: aes128gcm\0", &mut cek)
        .map_err(|_| AppError::Internal("HKDF expand cek".into()))?;
    let mut nonce = [0u8; AES128GCM_NONCE_LEN];
    h2.expand(b"Content-Encoding: nonce\0", &mut nonce)
        .map_err(|_| AppError::Internal("HKDF expand nonce".into()))?;

    // 5. Padding : 1 byte 0x02 (delimiter=end)
    // Le contenu chiffré = plaintext || padding_delimiter
    let mut plaintext = Vec::with_capacity(payload.len() + 1);
    plaintext.extend_from_slice(payload);
    plaintext.push(0x02);

    let cipher = Aes128Gcm::new(&cek.into());
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| AppError::Internal("aes-gcm encrypt failed".into()))?;
    // ciphertext already includes the 16-byte tag suffix.

    // 6. Header RFC 8188 §2.1 :
    //    salt(16) || rs(4 big-endian) || idlen(1) || keyid(idlen)
    //    keyid = éphémère public key (65 bytes)
    let rs: u32 = (ciphertext.len() + 1) as u32; // record size = tag + padding + data (single record)
    let rs_arr: [u8; 4] = ((ciphertext.len() + AES128GCM_TAG_LEN) as u32).to_be_bytes(); // total record size incl tag
    let _ = rs; // silence unused warning path
    let mut body = Vec::with_capacity(16 + 4 + 1 + as_pub_bytes.len() + ciphertext.len());
    body.extend_from_slice(&salt);
    body.extend_from_slice(&rs_arr);
    body.push(as_pub_bytes.len() as u8);
    body.extend_from_slice(&as_pub_bytes);
    body.extend_from_slice(&ciphertext);
    Ok((body, as_pub_bytes))
}

fn build_vapid_jwt(vapid: &VapidConfig, audience: &str) -> Result<String, AppError> {
    let header = serde_json::json!({"typ": "JWT", "alg": "ES256"});
    let exp = chrono::Utc::now().timestamp() + 12 * 3600;
    let claims = serde_json::json!({
        "aud": audience,
        "exp": exp,
        "sub": vapid.subject,
    });
    let header_b64 = B64_URL.encode(serde_json::to_vec(&header).unwrap());
    let claims_b64 = B64_URL.encode(serde_json::to_vec(&claims).unwrap());
    let signing_input = format!("{header_b64}.{claims_b64}");

    let priv_bytes = B64_URL
        .decode(vapid.private_key_b64.trim_end_matches('='))
        .map_err(|e| AppError::Internal(format!("vapid privkey decode: {e}")))?;
    if priv_bytes.len() != 32 {
        return Err(AppError::Internal("vapid privkey must be 32 bytes".into()));
    }
    let signing_key = SigningKey::from_bytes(priv_bytes.as_slice().into())
        .map_err(|e| AppError::Internal(format!("vapid signing key: {e}")))?;
    let sig: Signature = signing_key.sign(signing_input.as_bytes());
    let sig_bytes = sig.to_bytes();
    let sig_b64 = B64_URL.encode(sig_bytes.as_slice());
    Ok(format!("{signing_input}.{sig_b64}"))
}

// Silence unused import warning
#[allow(dead_code)]
fn _hmac_hint() -> impl Mac {
    <hmac::Hmac<Sha256> as HmacKeyInit>::new_from_slice(&[0u8; 32]).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_origin() {
        assert_eq!(
            origin_from_endpoint("https://fcm.googleapis.com/fcm/send/abc"),
            Some("https://fcm.googleapis.com".into())
        );
        assert_eq!(
            origin_from_endpoint("https://push.mozilla.com/somepath?q=1"),
            Some("https://push.mozilla.com".into())
        );
    }
}
