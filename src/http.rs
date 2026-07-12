//! HTTP client abstraction layer
//!
//! This module provides trait abstractions for HTTP clients, allowing postgrest-rs
//! to work with different HTTP implementations (reqwest, mock clients, etc.).
//!
//! # Design Philosophy
//!
//! The abstraction is designed to:
//! - Support the builder pattern used by reqwest
//! - Enable pure in-memory mock clients for testing (no Tokio reactor needed)
//! - Maintain backward compatibility with existing reqwest-based code
//! - Be minimal and focused only on what postgrest-rs actually needs
//!
//! # Usage with reqwest (default)
//!
//! ```rust
//! use postgrest::Postgrest;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Works exactly as before - reqwest::Client is the default
//! let client = Postgrest::new("https://api.example.com");
//! let response = client.from("users").select("*").execute().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Usage with MockClient (for testing)
//!
//! ```rust
//! # #[cfg(feature = "mock")]
//! use postgrest::{PostgrestGeneric, mock::MockClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # #[cfg(feature = "mock")]
//! # {
//! let mut mock_client = MockClient::new();
//! mock_client.mock_response(
//!     "GET",
//!     "/users",
//!     200,
//!     r#"[{"id": 1, "name": "Alice"}]"#
//! );
//!
//! let client = PostgrestGeneric::new_with_client("https://api.example.com", mock_client);
//! let response = client.from("users").select("*").execute().await?;
//! # }
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Method,
};
use std::error::Error as StdError;

/// Trait for HTTP clients that can execute requests.
///
/// This trait abstracts over different HTTP client implementations, allowing
/// postgrest-rs to work with reqwest (production) or mock clients (testing).
///
/// # Design Notes
///
/// - Uses `async_trait` for async methods
/// - Returns a request builder that supports method chaining
/// - Generic over error types to support different client implementations
#[async_trait]
pub trait HttpClient: Clone + Send + Sync + 'static {
    /// The type of request builder this client creates
    type RequestBuilder: HttpRequestBuilder;

    /// The error type for this client
    type Error: StdError + Send + Sync + 'static;

    /// Create a new HTTP request with the given method and URL.
    ///
    /// Returns a request builder that can be further configured before execution.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let request = client.request(Method::GET, "https://api.example.com/users");
    /// ```
    fn request(&self, method: Method, url: impl AsRef<str>) -> Self::RequestBuilder;
}

/// Trait for HTTP request builders that support method chaining.
///
/// This trait abstracts the builder pattern used by reqwest and other HTTP clients.
/// Each method returns `Self` to enable fluent chaining:
///
/// ```ignore
/// client.request(Method::GET, url)
///     .headers(headers)
///     .query(&params)
///     .body(body)
///     .send()
///     .await?
/// ```
#[async_trait]
pub trait HttpRequestBuilder: Send + 'static {
    /// The type of response returned by this builder
    type Response: HttpResponse;

    /// The error type for request execution
    type Error: StdError + Send + Sync + 'static;

    /// Add headers to the request.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut headers = HeaderMap::new();
    /// headers.insert("Authorization", HeaderValue::from_static("Bearer token"));
    /// request.headers(headers)
    /// ```
    fn headers(self, headers: HeaderMap) -> Self;

    /// Add query parameters to the request.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let params = vec![("page", "1"), ("limit", "10")];
    /// request.query(&params)
    /// ```
    fn query<T: serde::Serialize + ?Sized>(self, query: &T) -> Self;

    /// Set the request body.
    ///
    /// # Example
    ///
    /// ```ignore
    /// request.body(r#"{"name": "Alice"}"#)
    /// ```
    fn body(self, body: String) -> Self;

    /// Set the request body from raw bytes.
    ///
    /// Used for binary payloads (e.g. Supabase Storage uploads) that are not
    /// valid UTF-8 and therefore cannot go through [`body`](Self::body).
    fn body_bytes(self, body: Vec<u8>) -> Self;

    /// Execute the HTTP request.
    ///
    /// This consumes the builder and returns a Future that resolves to the response.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails (network error, server error, etc.)
    async fn send(self) -> Result<Self::Response, Self::Error>;
}

/// Trait for HTTP responses.
///
/// This trait provides a minimal interface for accessing response data.
/// It intentionally only exposes methods that postgrest-rs actually uses.
#[async_trait]
pub trait HttpResponse: Send + 'static {
    /// The error type for response operations
    type Error: StdError + Send + Sync + 'static;

    /// Get the HTTP status code of the response.
    ///
    /// # Example
    ///
    /// ```ignore
    /// assert_eq!(response.status().as_u16(), 200);
    /// ```
    fn status(&self) -> reqwest::StatusCode;

    /// Get the response headers.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let content_type = response.headers().get("content-type");
    /// ```
    fn headers(&self) -> &HeaderMap<HeaderValue>;

    /// Consume the response and return the body as text.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the body fails.
    async fn text(self) -> Result<String, Self::Error>;

    /// Consume the response and return the body as bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the body fails.
    async fn bytes(self) -> Result<Vec<u8>, Self::Error>;
}

// ============================================================================
// reqwest Implementation
// ============================================================================

/// Implement HttpClient for reqwest::Client
///
/// This enables reqwest to work through our trait abstraction with zero overhead.
/// All methods are simple pass-throughs to the underlying reqwest types.
#[async_trait]
impl HttpClient for reqwest::Client {
    type RequestBuilder = reqwest::RequestBuilder;
    type Error = reqwest::Error;

    fn request(&self, method: Method, url: impl AsRef<str>) -> Self::RequestBuilder {
        // Delegate directly to reqwest's implementation
        reqwest::Client::request(self, method, url.as_ref())
    }
}

/// Implement HttpRequestBuilder for reqwest::RequestBuilder
///
/// This is a simple pass-through wrapper that maintains reqwest's builder pattern.
#[async_trait]
impl HttpRequestBuilder for reqwest::RequestBuilder {
    type Response = reqwest::Response;
    type Error = reqwest::Error;

    fn headers(self, headers: HeaderMap) -> Self {
        // Delegate to reqwest's builder
        reqwest::RequestBuilder::headers(self, headers)
    }

    fn query<T: serde::Serialize + ?Sized>(self, query: &T) -> Self {
        // Delegate to reqwest's builder
        reqwest::RequestBuilder::query(self, query)
    }

    fn body(self, body: String) -> Self {
        // Delegate to reqwest's builder
        reqwest::RequestBuilder::body(self, body)
    }

    fn body_bytes(self, body: Vec<u8>) -> Self {
        // reqwest accepts any `Into<Body>`, including `Vec<u8>`.
        reqwest::RequestBuilder::body(self, body)
    }

    async fn send(self) -> Result<Self::Response, Self::Error> {
        // Delegate to reqwest's send
        reqwest::RequestBuilder::send(self).await
    }
}

/// Implement HttpResponse for reqwest::Response
///
/// This provides access to the response through our trait interface.
#[async_trait]
impl HttpResponse for reqwest::Response {
    type Error = reqwest::Error;

    fn status(&self) -> reqwest::StatusCode {
        reqwest::Response::status(self)
    }

    fn headers(&self) -> &HeaderMap<HeaderValue> {
        reqwest::Response::headers(self)
    }

    async fn text(self) -> Result<String, Self::Error> {
        reqwest::Response::text(self).await
    }

    async fn bytes(self) -> Result<Vec<u8>, Self::Error> {
        reqwest::Response::bytes(self).await.map(|b| b.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time test: ensure reqwest::Client implements HttpClient
    #[allow(dead_code)]
    fn assert_reqwest_implements_http_client() {
        fn is_http_client<T: HttpClient>() {}
        is_http_client::<reqwest::Client>();
    }

    // Compile-time test: ensure reqwest::RequestBuilder implements HttpRequestBuilder
    #[allow(dead_code)]
    fn assert_reqwest_request_builder_implements_trait() {
        fn is_request_builder<T: HttpRequestBuilder>() {}
        is_request_builder::<reqwest::RequestBuilder>();
    }

    // Compile-time test: ensure reqwest::Response implements HttpResponse
    #[allow(dead_code)]
    fn assert_reqwest_response_implements_trait() {
        fn is_response<T: HttpResponse>() {}
        is_response::<reqwest::Response>();
    }
}
