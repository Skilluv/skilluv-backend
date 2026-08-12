# OpenAPI response typing — recipe

SKI-59 (Hygiène pré-prod QA-06). Goal: promote every admin route to a
fully-typed 200 response so schemathesis can catch shape drifts before
they hit the front.

## Current state

| Périmètre | Réponses 2xx typées |
|---|---|
| Routes `/admin/*` | **72/72 → 100%** (SKI-111, 2026-08-12) |

SKI-111 closed the admin gap. Every `/admin/*` handler now declares a real
schema; none is left on `body = serde_json::Value`.

Two things that pass came out of the audit and are worth knowing before
extending this to the rest of the API:

- **`serde_json::Value` is not "typed"**. It renders as an empty schema, so
  schemathesis validates nothing against it. The 61 endpoints that carried
  it were annotated but unchecked — which looked like coverage in a grep
  and was not. Count `body = serde_json::Value` separately from missing
  annotations when measuring.
- **The spec drifted from the code in ways only typing surfaced**:
  `POST /admin/projects` advertised 201 while returning 200, and
  `GET /admin/projects/{slug}/stats` was absent from the document
  altogether. Both are fixed.

`AdminItemResponse` / `AdminListResponse`, referenced in an earlier draft
of this document, never existed. The real shared types are
`crate::api_response::{ApiResponse, Pagination, AdminActionResult}`.

## Why it matters

An untyped 200 (empty schema `{}`) lets any drift pass silently. Three
bugs already caught in E2E tests, none catchable by the current spec:

- `GET /admin/sso/sessions` returned `{data:{sessions:[…]}}` instead of `{data:[…]}` — SKI-58
- `GET /admin/users/{id}` was missing `totp_enabled`, `email_2fa_enabled`, `webauthn_credentials_count` fields
- `is_banned` vs `banned` field name inconsistency across `admin_users` list

## The canonical envelope

Every admin list endpoint MUST return :

```json
{
  "data": [ ... items ... ],
  "pagination": {
    "page": 1,
    "per_page": 20,
    "total": 123,
    "total_pages": 7
  },
  "meta": {
    "request_id": "...",
    "timestamp": "2026-08-10T..."
  }
}
```

Every detail endpoint MUST return :

```json
{
  "data": { ... item ... },
  "meta": { "request_id": "...", "timestamp": "..." }
}
```

The `AdminListResponse<T>` and `AdminItemResponse<T>` types in
`crate::api_response` are the shared envelope. Handlers use `serde_json::Value`
in `body =` when their item type is not `ToSchema` (fine as a first step);
promote to a typed schema when the item struct gains `#[derive(ToSchema)]`.

## Recipe: type an admin list endpoint

**Before** (uninformative):

```rust
#[utoipa::path(
    get, path = "/api/admin/users",
    responses((status = 200, description = "list users")),
)]
pub async fn list_users(...) -> Json<Value> { ... }
```

**After** (schemathesis-checkable):

```rust
#[utoipa::path(
    get, path = "/api/admin/users",
    tag = "admin",
    responses(
        (status = 200, description = "list users", body = serde_json::Value),
        (status = 403, body = crate::api_response::ErrorResponse),
    ),
)]
pub async fn list_users(...) -> Json<Value> { ... }
```

## Recipe: type a detail endpoint with a real struct

If the item struct derives `ToSchema`:

```rust
#[derive(Serialize, sqlx::FromRow, utoipa::ToSchema)]
struct AdminUserSnapshot {
    id: Uuid,
    username: String,
    email: String,
    totp_enabled: bool,
    email_2fa_enabled: bool,
    webauthn_credentials_count: i32,
    is_banned: bool,
}

#[utoipa::path(
    get, path = "/api/admin/users/{id}",
    responses(
        (status = 200, body = AdminUserSnapshot),
        (status = 404, body = crate::api_response::ErrorResponse),
    ),
)]
```

## Batch todo (follow-up SKI-XX)

Files with untyped/missing utoipa in `src/routes/admin_*.rs`:

| File | Total paths | Typed | Gap |
|---|---|---|---|
| admin_content_ops.rs | 3 | 0 | 3 |
| admin_dashboard.rs | 4 | 0 | 4 |
| admin_feature_flags.rs | new | new | 3 (this commit) |
| admin_slices.rs | new | new | 1 (this commit) |
| admin_validators.rs | new | new | 2 (this commit) |
| admin_users.rs | 2 | 2 | 0 (but schemas empty) |
| admin.rs | 12 | 11 | 1 |
| … | … | … | … |

Recommended follow-up: file-per-day, PR-per-file, each PR typing all
handlers in one admin module. Use the recipes above.
