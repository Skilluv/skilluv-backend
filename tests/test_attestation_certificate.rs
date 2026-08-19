//! What an attestation looks like when somebody shows it.
//!
//! The two surfaces a proof travels by: a card, because a link with no
//! picture is not opened, and a printable sheet, because a candidate attaches
//! a document to an application.
//!
//! The claim this suite holds is the awkward one: **a revoked attestation
//! still renders, and says so**. A 404 would leave whoever is holding an old
//! copy believing it.

mod common;
use common::TestApp;
use uuid::Uuid;

/// An attestation of a given basis, written straight to the table — the
/// issuing paths are covered by their own suites, and what is under test here
/// is the rendering.
async fn an_attestation(app: &TestApp, username: &str, basis: &str, code: &str) -> Uuid {
    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_one(&app.db)
        .await
        .unwrap();

    // An artefact basis must link a deliverable — a CHECK constraint that has
    // been widened once per domain since 0178, and which is the whole point of
    // the distinction the sheet draws. An editorial featuring links nothing,
    // because it rests on somebody's judgement rather than on a file.
    let linked: Vec<Uuid> = if basis.starts_with("featured_") {
        vec![]
    } else {
        vec![Uuid::new_v4()]
    };

    sqlx::query_scalar(
        r#"
        INSERT INTO attestations
            (user_id, attestation_type, title, description, basis,
             verification_code, linked_deliverable_ids)
        VALUES ($1, 'artefact',
                'Identité complète pour une coopérative de transformation d''anacarde',
                'Logotype, palette et guidelines, livrés avec leurs sources.',
                $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(basis)
    .bind(code)
    .bind(&linked)
    .fetch_one(&app.db)
    .await
    .expect("attestation")
}

#[tokio::test]
async fn the_card_is_a_png_a_crawler_can_render() {
    let app = TestApp::spawn().await;
    app.register_user("cert_owner").await;
    an_attestation(&app, "cert_owner", "design_deliverable_validated", "AAAA111122").await;

    // Public and unauthenticated: the callers are the crawlers of X and
    // LinkedIn, which follow `og:image` without cookies.
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/api/attestations/verify/AAAA111122/card.png",
            app.addr
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "image/png",
        "a card served as anything else renders as nothing"
    );

    let bytes = resp.bytes().await.unwrap();
    // The PNG magic number: proof it was actually rasterised rather than a
    // placeholder that happens to be served with the right header.
    assert_eq!(&bytes[..4], b"\x89PNG", "not a PNG");
    assert!(bytes.len() > 5_000, "suspiciously small: {}", bytes.len());
}

#[tokio::test]
async fn the_sheet_is_a4_svg_with_the_code_printed_on_it() {
    let app = TestApp::spawn().await;
    app.register_user("cert_printer").await;
    an_attestation(&app, "cert_printer", "design_deliverable_validated", "BBBB222233").await;

    let resp = reqwest::Client::new()
        .get(format!(
            "{}/api/attestations/verify/BBBB222233/certificate.svg",
            app.addr
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 200);
    let svg = resp.text().await.unwrap();

    // A4 at 96 dpi, so the browser prints it to a page rather than scaling it
    // to something arbitrary.
    assert!(svg.contains(r#"width="794""#), "{svg}");
    assert!(svg.contains(r#"height="1123""#));

    // The code is printed as well as encoded in the QR: a sheet photocopied
    // badly enough to lose the QR is still checkable by hand.
    assert!(svg.contains("BBBB222233"), "the code is not on the sheet");
    assert!(svg.contains("cert_printer"));
}

#[tokio::test]
async fn a_revoked_attestation_still_renders_and_says_so() {
    let app = TestApp::spawn().await;
    app.register_user("cert_revoked").await;
    let id = an_attestation(&app, "cert_revoked", "design_deliverable_validated", "CCCC333344").await;

    sqlx::query("UPDATE attestations SET revoked_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();

    // Somebody holding an old copy has to be able to find out that it no
    // longer holds. A 404 would leave them believing it.
    let sheet = reqwest::Client::new()
        .get(format!(
            "{}/api/attestations/verify/CCCC333344/certificate.svg",
            app.addr
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(sheet.status().as_u16(), 200);
    assert!(sheet.text().await.unwrap().contains("RÉVOQUÉE"));
}

#[tokio::test]
async fn an_editorial_featuring_does_not_look_like_a_verified_artefact() {
    let app = TestApp::spawn().await;
    app.register_user("cert_featured").await;
    an_attestation(&app, "cert_featured", "featured_designer", "DDDD444455").await;

    let featured = reqwest::Client::new()
        .get(format!(
            "{}/api/attestations/verify/DDDD444455/certificate.svg",
            app.addr
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // The featuring rests on somebody's judgement; the other bases rest on an
    // artefact. Reading the two as interchangeable is exactly what a reader
    // must not do.
    assert!(featured.contains("MISE EN AVANT ÉDITORIALE"), "{featured}");
    assert!(featured.contains("choix éditorial"));
    assert!(!featured.contains("TRAVAIL VÉRIFIÉ"));
}

#[tokio::test]
async fn an_unknown_code_is_a_404_on_both_surfaces() {
    let app = TestApp::spawn().await;

    for path in ["card.png", "certificate.svg"] {
        let resp = reqwest::Client::new()
            .get(format!(
                "{}/api/attestations/verify/ZZZZ999900/{path}",
                app.addr
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 404, "{path}");
    }
}

#[tokio::test]
async fn a_card_is_not_cached_for_a_year() {
    let app = TestApp::spawn().await;
    app.register_user("cert_cache").await;
    an_attestation(&app, "cert_cache", "design_deliverable_validated", "EEEE555566").await;

    let resp = reqwest::Client::new()
        .get(format!(
            "{}/api/attestations/verify/EEEE555566/card.png",
            app.addr
        ))
        .send()
        .await
        .unwrap();

    // An attestation can be revoked. A card cached as immutable would keep
    // saying it holds long after it stopped.
    let cache = resp
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(!cache.contains("immutable"), "{cache}");
    assert!(cache.contains("max-age=3600"), "{cache}");
}
