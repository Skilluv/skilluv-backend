//! The middleware that recognises a research token, and how the rate limiter
//! finds out.
//!
//! ## Why a task-local and not a request extension
//!
//! The obvious design is `req.extensions_mut().insert(ResearchMode)`, and it
//! does not work here. `RateLimiter::check` is not a layer — it is a function
//! called from inside a hundred and some handlers, with the arguments the
//! handler chose, and it never sees the request. Making it see one would have
//! meant editing every call site to thread an extractor through, which is a
//! hundred edits where one of them gets forgotten and silently keeps the low
//! ceiling.
//!
//! A task-local works because axum runs each request's handler inside the
//! future this middleware wraps, so a value scoped here is visible to anything
//! that runs underneath — including a plain function nobody passed anything to.
//! Outside a scope it reads as absent rather than panicking, which is what
//! makes the limiter behave identically in tests and in the workers.
//!
//! ## What it deliberately does not do
//!
//! It does not authenticate. A valid token identifies a person for the audit
//! trail and grants nothing, and an invalid one is ignored rather than refused
//! — answering 401 to a bad token would turn this header into an oracle for
//! which tokens exist.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;
use crate::services::security_research::{self, ResearchMode};

/// The header a researcher sets. The username variant in the published scope
/// (`X-Security-Research`) is documentation for whoever reads the logs; this
/// one is the secret and the only one that changes behaviour.
pub const TOKEN_HEADER: &str = "X-Security-Research-Token";

/// The companion header from the scope document. Read only to be logged: a
/// researcher writing their handle in it is doing the courteous thing, and the
/// operator reading an access log wants to see it.
pub const HANDLE_HEADER: &str = "X-Security-Research";

tokio::task_local! {
    static RESEARCH_MODE: Option<ResearchMode>;
}

/// Whether the request being handled right now is declared research.
///
/// Reads as `None` outside a request — in a worker, in a test that calls a
/// service directly — which is the correct answer there.
pub fn current() -> Option<ResearchMode> {
    RESEARCH_MODE.try_with(|m| *m).ok().flatten()
}

/// What to multiply a rate limit ceiling by for this request.
///
/// One when there is no token, which is every request that is not somebody's
/// declared testing.
pub fn rate_limit_multiplier() -> u64 {
    if current().is_some() {
        security_research::RATE_LIMIT_MULTIPLIER
    } else {
        1
    }
}

/// Resolve the token, then run the rest of the stack inside its scope.
pub async fn resolve(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let mode = match presented {
        Some(token) => security_research::verify(&state.db, &token).await,
        None => None,
    };

    if let Some(mode) = mode {
        let handle = request
            .headers()
            .get(HANDLE_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        // Tagged on the span so an access log can be filtered to one
        // researcher's traffic, which is the difference between reviewing an
        // afternoon of testing and reviewing an incident.
        tracing::info!(
            security_research = true,
            researcher = %mode.user_id,
            declared_handle = %handle,
            "request under a research token"
        );

        let mut redis = state.redis.clone();
        let db = state.db.clone();
        // Counting must not delay the response, and its failure must not
        // affect it. The abuse rule lives in here, which is why it is spawned
        // rather than skipped.
        tokio::spawn(async move {
            security_research::record_use(&db, &mut redis, mode).await;
        });
    }

    RESEARCH_MODE.scope(mode, next.run(request)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_a_request_there_is_no_research_mode() {
        assert!(current().is_none());
        assert_eq!(rate_limit_multiplier(), 1);
    }

    #[tokio::test]
    async fn inside_a_scope_the_multiplier_applies() {
        let mode = ResearchMode {
            token_id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
        };
        RESEARCH_MODE
            .scope(Some(mode), async {
                assert!(current().is_some());
                assert_eq!(
                    rate_limit_multiplier(),
                    security_research::RATE_LIMIT_MULTIPLIER
                );
            })
            .await;
        // And it does not leak out of the scope.
        assert!(current().is_none());
    }
}
