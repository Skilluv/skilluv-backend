//! P26 v2 SKI-116 / SKI-117 — public SVG badges (shields.io-compatible).
//!
//! Two flavours, same renderer:
//!
//!   GET /badge/user/{username}/validated.svg
//!     → "Skilluv | 12 validated · ranger"
//!
//!   GET /badge/repo/{owner}/{name}/validated.svg
//!     → "Skilluv | 8 challenges validated"
//!
//! Users paste the URL in their GitHub profile README; maintainers paste
//! it in their repo README. Passive marketing / visibility for Skilluv
//! and asymmetric valorisation for the badge holder.
//!
//! ─── Design decisions ─────────────────────────────────────────────
//!
//! - **Server-rendered SVG**, no dependency on shields.io / third parties.
//!   The template is a single format string with the two dynamic labels;
//!   the layout matches shields.io's flat style (110×20 canvas).
//! - **Cache-Control: public, max-age=3600** — badges are cheap to
//!   compute but hit-per-README-render adds up. One-hour freshness is
//!   enough for a slowly-growing counter.
//! - **Missing user / repo → still 200** with count=0. A 404 would
//!   render as a broken image on GitHub which is worse UX than "just
//!   started, keep going".
//! - **XML-escape the dynamic label** so a username containing `<` or `&`
//!   cannot inject markup. shields.io does the same.

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;

use crate::AppState;
use crate::errors::AppError;

pub fn badge_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/badge/user/{username}/validated.svg",
            get(user_validated_svg),
        )
        .route(
            "/badge/repo/{owner}/{name}/validated.svg",
            get(repo_validated_svg),
        )
}

// ─── Renderer (pure, testable) ─────────────────────────────────────

const BADGE_LEFT: &str = "Skilluv";

/// Render a shields.io-style badge. The left side is fixed to
/// "Skilluv"; the right side is the caller-supplied label + count.
///
/// Widths are computed with a heuristic (7px per char, 10px padding)
/// that matches shields.io within a few pixels — good enough for
/// README embedding where font metrics differ across viewers anyway.
pub fn render_badge_svg(right_label: &str) -> String {
    let left = BADGE_LEFT;
    let right = xml_escape(right_label);
    let left_w = 10 + 7 * left.chars().count() as u32;
    let right_w = 10 + 7 * right.chars().count() as u32;
    let total_w = left_w + right_w;

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="20" role="img" aria-label="{left}: {right}">
  <linearGradient id="s" x2="0" y2="100%">
    <stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
    <stop offset="1" stop-opacity=".1"/>
  </linearGradient>
  <clipPath id="r"><rect width="{total_w}" height="20" rx="3" fill="#fff"/></clipPath>
  <g clip-path="url(#r)">
    <rect width="{left_w}" height="20" fill="#555"/>
    <rect x="{left_w}" width="{right_w}" height="20" fill="#2E7D32"/>
    <rect width="{total_w}" height="20" fill="url(#s)"/>
  </g>
  <g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" font-size="11">
    <text x="{lx}" y="15">{left}</text>
    <text x="{rx}" y="15">{right}</text>
  </g>
</svg>"##,
        lx = left_w / 2,
        rx = left_w + right_w / 2,
    )
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

// ─── Handlers ─────────────────────────────────────────────────────

fn svg_response(body: String) -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml; charset=utf-8"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=3600"),
            ),
        ],
        body,
    )
}

/// SKI-116 — user badge. Counts slices where the user is the challenger
/// AND status ∈ {validated, merged} (both count as success — merged is
/// the bonus tier of a validated slice).
#[utoipa::path(
    get, path = "/api/badge/user/{username}/validated.svg", tag = "public",
    params(("username" = String, Path, description = "Whose badge")),
    responses(
        (status = 200, description = "An SVG badge, cacheable, for a README"),
    ),
)]
pub async fn user_validated_svg(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let row: Option<(i64, Option<String>)> = sqlx::query_as(
        r#"
        SELECT
            (SELECT COUNT(*)::bigint
               FROM project_slices s
              WHERE s.claimed_by_user_id = u.id
                AND s.status IN ('validated','merged')) AS n,
            (SELECT r.rank FROM user_ranks r WHERE r.user_id = u.id) AS rank
          FROM users u
         WHERE u.username = $1
        "#,
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await?;

    let (count, rank) = row.unwrap_or((0, None));
    let rank_suffix = rank
        .as_deref()
        .map(|r| format!(" · {r}"))
        .unwrap_or_default();
    let label = format!("{count} validated{rank_suffix}");
    Ok(svg_response(render_badge_svg(&label)))
}

/// SKI-117 — repo badge. Counts slices attached to the project matching
/// (owner, name) with `status ∈ {validated, merged}`.
#[utoipa::path(
    get, path = "/api/badge/repo/{owner}/{name}/validated.svg", tag = "public",
    params(("owner" = String, Path, description = "Repository owner"), ("name" = String, Path, description = "Repository name")),
    responses(
        (status = 200, description = "An SVG badge, cacheable, for a README"),
    ),
)]
pub async fn repo_validated_svg(
    State(state): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
          FROM project_slices s
          JOIN projects p ON p.id = s.project_id
         WHERE p.github_repo_owner = $1
           AND p.github_repo_name = $2
           AND s.status IN ('validated','merged')
        "#,
    )
    .bind(&owner)
    .bind(&name)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let label = format!("{count} challenges validated");
    Ok(svg_response(render_badge_svg(&label)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_covers_all_meta() {
        assert_eq!(
            xml_escape(r#"<a href="x">'&</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&apos;&amp;&lt;/a&gt;"
        );
    }

    #[test]
    fn render_produces_valid_shape() {
        let s = render_badge_svg("5 validated · ranger");
        assert!(s.starts_with("<svg"));
        assert!(s.ends_with("</svg>"));
        assert!(s.contains("Skilluv"));
        assert!(s.contains("5 validated · ranger"));
    }

    #[test]
    fn render_escapes_hostile_label() {
        // Username can theoretically contain characters allowed by our
        // schema (letters/digits/hyphen). Escaping is defense-in-depth
        // in case the DB constraint changes.
        let s = render_badge_svg("<script>alert(1)</script>");
        assert!(!s.contains("<script>"));
        assert!(s.contains("&lt;script&gt;"));
    }
}
