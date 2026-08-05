//! `ValidatedQuery<T>` : wrapper autour d'axum `Query<T>` qui convertit
//! les erreurs de deserialisation en `AppError::Validation` (JSON conforme
//! au schema d'erreur documente), au lieu du `text/plain` que renvoie
//! axum par defaut.
//!
//! Motivation : schemathesis `content_type_conformance` flag toute
//! reponse qui ne matche pas le Content-Type declare dans le schema
//! OpenAPI (application/json partout via CommonErrorResponsesAddon). La
//! reponse text/plain du `Query::from_request_parts` rejection ne
//! matchait pas, causant un fail sur tout endpoint public utilisant
//! `Query<T>` avec des query params requis.
//!
//! Usage : remplacer `Query<T>` par `ValidatedQuery<T>` sur les
//! endpoints exposes au fuzzer. Meme API (`.0` pour acceder au T
//! sous-jacent), meme deserialisation serde.

use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::errors::AppError;

pub struct ValidatedQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(t)) => Ok(ValidatedQuery(t)),
            Err(rejection) => {
                // QueryRejection expose un message user-friendly via
                // Display — on le reformate en Validation pour que la
                // reponse soit application/json.
                let msg = match &rejection {
                    QueryRejection::FailedToDeserializeQueryString(e) => {
                        format!("Invalid query parameters: {e}")
                    }
                    _ => format!("Query rejection: {rejection}"),
                };
                Err(AppError::Validation(msg))
            }
        }
    }
}
