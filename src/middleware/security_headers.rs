use axum::http::{HeaderValue, Request, Response, header};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

/// Layer that adds security headers to all responses.
#[derive(Clone)]
pub struct SecurityHeadersLayer;

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersMiddleware { inner }
    }
}

#[derive(Clone)]
pub struct SecurityHeadersMiddleware<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for SecurityHeadersMiddleware<S>
where
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut response = inner.call(request).await?;
            let headers = response.headers_mut();

            headers.insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
            headers.insert(
                header::X_XSS_PROTECTION,
                HeaderValue::from_static("1; mode=block"),
            );
            headers.insert(
                header::STRICT_TRANSPORT_SECURITY,
                HeaderValue::from_static("max-age=31536000; includeSubDomains"),
            );
            headers.insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("default-src 'self'"),
            );

            // Where to send a vulnerability, on every response.
            //
            // `security.txt` (RFC 9116) is the canonical place for this and is
            // served under /.well-known; these two headers are for the case
            // that actually matters, which is somebody who has just found
            // something while looking at an API response and has no reason to
            // go and read a file. The cost is forty bytes.
            headers.insert(
                "X-Security-Contact",
                HeaderValue::from_static("mailto:security@skill-uv.com"),
            );
            headers.insert(
                "X-Security-Policy",
                HeaderValue::from_static("https://skill-uv.com/security"),
            );

            Ok(response)
        })
    }
}
