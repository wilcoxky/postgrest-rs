//! Pure in-memory mock HTTP client for testing
//!
//! This module provides a mock HTTP client that can be used in tests without requiring
//! a Tokio reactor or actual network connections. It follows an httpmock-inspired API
//! for configuring mock responses.
//!
//! # Example Usage
//!
//! ```
//! use postgrest::{PostgrestGeneric, mock::MockClient};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut mock_client = MockClient::new();
//!
//! // Configure a mock response
//! mock_client
//!     .mock("GET", "/users")
//!     .match_header("Authorization", "Bearer token")
//!     .match_query("id", "eq.1")
//!     .match_query("select", "*")
//!     .respond_with(200, r#"[{"id": 1, "name": "Alice"}]"#);
//!
//! // Use with PostgrestGeneric
//! let client = PostgrestGeneric::new_with_client("http://api.test", mock_client);
//! let response = client
//!     .from("users")
//!     .eq("id", "1")
//!     .select("*")
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Method, StatusCode,
};
use std::collections::HashMap;
use std::fmt;

use crate::http::{HttpClient, HttpRequestBuilder, HttpResponse};

// ============================================================================
// Core Structures
// ============================================================================

/// A pure in-memory mock HTTP client for testing.
///
/// The MockClient stores configured mock responses and matches incoming requests
/// against them. When a request is made, it searches through the registered mocks
/// and returns the first matching response.
#[derive(Clone, Debug)]
pub struct MockClient {
    mocks: Vec<MockRule>,
    default_response: Option<MockedResponse>,
}

/// A rule that matches requests and returns a configured response.
#[derive(Clone, Debug)]
struct MockRule {
    matcher: RequestMatcher,
    response: MockedResponse,
}

/// Matches incoming requests based on method, path, headers, and query parameters.
#[derive(Clone, Debug)]
struct RequestMatcher {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    query_params: HashMap<String, String>,
}

/// A mocked HTTP response with status, body, and headers.
#[derive(Clone, Debug)]
struct MockedResponse {
    status: u16,
    body: String,
    headers: HeaderMap,
}

// ============================================================================
// MockClient Implementation
// ============================================================================

impl MockClient {
    /// Creates a new MockClient with no configured mocks.
    ///
    /// By default, unmatched requests will return a 404 response.
    pub fn new() -> Self {
        Self {
            mocks: Vec::new(),
            default_response: None,
        }
    }

    /// Starts configuring a new mock response.
    ///
    /// Returns a `MockBuilder` that allows you to specify matching conditions
    /// and the response to return.
    ///
    /// # Example
    ///
    /// ```
    /// # use postgrest::mock::MockClient;
    /// let mut mock = MockClient::new();
    /// mock.mock("GET", "/users")
    ///     .match_query("id", "eq.1")
    ///     .respond_with(200, r#"[{"id": 1}]"#);
    /// ```
    pub fn mock(&mut self, method: &str, path: &str) -> MockBuilder<'_> {
        MockBuilder {
            client: self,
            matcher: RequestMatcher {
                method: method.to_uppercase(),
                path: path.to_string(),
                headers: HashMap::new(),
                query_params: HashMap::new(),
            },
        }
    }

    /// Sets a default response for requests that don't match any mocks.
    ///
    /// If not set, unmatched requests return a 404 response.
    pub fn set_default_response(&mut self, status: u16, body: &str) {
        self.default_response = Some(MockedResponse {
            status,
            body: body.to_string(),
            headers: HeaderMap::new(),
        });
    }

    /// Finds a matching mock for the given request.
    fn find_mock(
        &self,
        method: &Method,
        url: &str,
        headers: &HeaderMap,
        query: &[(String, String)],
    ) -> MockedResponse {
        // Try to find a matching mock
        for rule in &self.mocks {
            if rule.matcher.matches(method.as_str(), url, headers, query) {
                return rule.response.clone();
            }
        }

        // Return default or 404
        self.default_response.clone().unwrap_or_else(|| MockedResponse {
            status: 404,
            body: format!("No mock found for {} {}", method, url),
            headers: HeaderMap::new(),
        })
    }
}

impl Default for MockClient {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// MockBuilder - Fluent API for configuring mocks
// ============================================================================

/// Builder for configuring mock responses with a fluent API.
///
/// Created by calling `MockClient::mock()`. Use the `match_*` methods to
/// specify conditions, then call `respond_with()` to set the response.
pub struct MockBuilder<'a> {
    client: &'a mut MockClient,
    matcher: RequestMatcher,
}

impl<'a> MockBuilder<'a> {
    /// Adds a header match condition.
    ///
    /// The request must have a header with the given name and value to match this mock.
    ///
    /// # Example
    ///
    /// ```
    /// # use postgrest::mock::MockClient;
    /// # let mut mock = MockClient::new();
    /// mock.mock("GET", "/users")
    ///     .match_header("Authorization", "Bearer token")
    ///     .respond_with(200, "[]");
    /// ```
    pub fn match_header(mut self, name: &str, value: &str) -> Self {
        self.matcher.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Adds a query parameter match condition.
    ///
    /// The request must have a query parameter with the given name and value to match this mock.
    ///
    /// # Example
    ///
    /// ```
    /// # use postgrest::mock::MockClient;
    /// # let mut mock = MockClient::new();
    /// mock.mock("GET", "/users")
    ///     .match_query("id", "eq.1")
    ///     .match_query("select", "*")
    ///     .respond_with(200, r#"[{"id": 1}]"#);
    /// ```
    pub fn match_query(mut self, name: &str, value: &str) -> Self {
        self.matcher.query_params.insert(name.to_string(), value.to_string());
        self
    }

    /// Sets the response for this mock and registers it with the client.
    ///
    /// # Example
    ///
    /// ```
    /// # use postgrest::mock::MockClient;
    /// # let mut mock = MockClient::new();
    /// mock.mock("POST", "/users")
    ///     .respond_with(201, r#"{"id": 1, "name": "Alice"}"#);
    /// ```
    pub fn respond_with(self, status: u16, body: &str) {
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );

        let response = MockedResponse {
            status,
            body: body.to_string(),
            headers,
        };

        self.client.mocks.push(MockRule {
            matcher: self.matcher,
            response,
        });
    }

    /// Sets the response with custom headers.
    pub fn respond_with_headers(self, status: u16, body: &str, headers: HeaderMap) {
        let response = MockedResponse {
            status,
            body: body.to_string(),
            headers,
        };

        self.client.mocks.push(MockRule {
            matcher: self.matcher,
            response,
        });
    }
}

// ============================================================================
// RequestMatcher - Logic for matching requests
// ============================================================================

impl RequestMatcher {
    /// Checks if this matcher matches the given request.
    ///
    /// All conditions must match (AND logic):
    /// - Method must match (case-insensitive)
    /// - Path must match exactly (query string ignored)
    /// - All specified headers must be present with matching values
    /// - All specified query parameters must be present with matching values
    fn matches(
        &self,
        method: &str,
        url: &str,
        headers: &HeaderMap,
        query: &[(String, String)],
    ) -> bool {
        // Check method
        if self.method != method.to_uppercase() {
            return false;
        }

        // Extract path from URL (everything before ?)
        let path = url.split('?').next().unwrap_or(url);
        if self.path != path {
            return false;
        }

        // Check all required headers are present
        for (header_name, header_value) in &self.headers {
            match headers.get(header_name) {
                Some(value) => {
                    if value.to_str().unwrap_or("") != header_value {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // Check all required query parameters are present
        let query_map: HashMap<&str, &str> = query
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        for (param_name, param_value) in &self.query_params {
            match query_map.get(param_name.as_str()) {
                Some(&value) => {
                    if value != param_value {
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }
}

// ============================================================================
// HttpClient Implementation for MockClient
// ============================================================================

#[async_trait]
impl HttpClient for MockClient {
    type RequestBuilder = MockRequestBuilder;
    type Error = MockHttpError;

    fn request(&self, method: Method, url: impl AsRef<str>) -> Self::RequestBuilder {
        MockRequestBuilder {
            client: self.clone(),
            method,
            url: url.as_ref().to_string(),
            headers: HeaderMap::new(),
            query: Vec::new(),
            body: String::new(),
        }
    }
}

// ============================================================================
// MockRequestBuilder - Implements HttpRequestBuilder
// ============================================================================

/// Mock implementation of an HTTP request builder.
///
/// This stores request configuration and returns a mocked response when `send()` is called.
pub struct MockRequestBuilder {
    client: MockClient,
    method: Method,
    url: String,
    headers: HeaderMap,
    query: Vec<(String, String)>,
    body: String,
}

#[async_trait]
impl HttpRequestBuilder for MockRequestBuilder {
    type Response = MockHttpResponse;
    type Error = MockHttpError;

    fn headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        // Parse the query into key-value pairs
        // serde_urlencoded gives us the serialized form
        if let Ok(query_string) = serde_urlencoded::to_string(query) {
            for pair in query_string.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    self.query.push((
                        urlencoding::decode(key).unwrap_or_default().to_string(),
                        urlencoding::decode(value).unwrap_or_default().to_string(),
                    ));
                }
            }
        }
        self
    }

    fn body(mut self, body: String) -> Self {
        self.body = body;
        self
    }

    async fn send(self) -> Result<Self::Response, Self::Error> {
        // Find matching mock
        let response = self.client.find_mock(&self.method, &self.url, &self.headers, &self.query);

        Ok(MockHttpResponse {
            status: StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK),
            headers: response.headers,
            body: response.body,
        })
    }
}

// ============================================================================
// MockHttpResponse - Implements HttpResponse
// ============================================================================

/// Mock implementation of an HTTP response.
pub struct MockHttpResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

#[async_trait]
impl HttpResponse for MockHttpResponse {
    type Error = MockHttpError;

    fn status(&self) -> StatusCode {
        self.status
    }

    fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.headers
    }

    async fn text(self) -> Result<String, Self::Error> {
        Ok(self.body)
    }

    async fn bytes(self) -> Result<Vec<u8>, Self::Error> {
        Ok(self.body.into_bytes())
    }
}

// ============================================================================
// MockHttpError - Error type for mock operations
// ============================================================================

/// Error type for mock HTTP operations.
///
/// Since MockClient operates in-memory, errors are rare and typically indicate
/// configuration issues rather than network problems.
#[derive(Debug, Clone)]
pub enum MockHttpError {
    /// An error occurred while processing the mock request
    MockError(String),
}

impl fmt::Display for MockHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MockHttpError::MockError(msg) => write!(f, "Mock error: {}", msg),
        }
    }
}

impl std::error::Error for MockHttpError {}

// ============================================================================
// Helper Functions
// ============================================================================

/// Creates a mocked JSON response with the appropriate content-type header.
///
/// # Example
///
/// ```
/// # use postgrest::mock::{MockClient, mock_json};
/// let mut mock = MockClient::new();
/// let response = mock_json(200, r#"{"id": 1, "name": "Alice"}"#);
/// ```
pub fn mock_json(status: u16, json: &str) -> (u16, String, HeaderMap) {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/json"),
    );
    (status, json.to_string(), headers)
}

/// Creates a mocked error response.
///
/// # Example
///
/// ```
/// # use postgrest::mock::{MockClient, mock_error};
/// let mut mock = MockClient::new();
/// let response = mock_error(500, "Internal server error");
/// ```
pub fn mock_error(status: u16, message: &str) -> (u16, String, HeaderMap) {
    let body = format!(r#"{{"error": "{}"}}"#, message);
    mock_json(status, &body)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_client_creation() {
        let mock = MockClient::new();
        assert_eq!(mock.mocks.len(), 0);
    }

    #[test]
    fn test_mock_builder_basic() {
        let mut mock = MockClient::new();
        mock.mock("GET", "/users").respond_with(200, "[]");
        assert_eq!(mock.mocks.len(), 1);
    }

    #[test]
    fn test_request_matcher_method() {
        let matcher = RequestMatcher {
            method: "GET".to_string(),
            path: "/users".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
        };

        assert!(matcher.matches("GET", "/users", &HeaderMap::new(), &[]));
        assert!(!matcher.matches("POST", "/users", &HeaderMap::new(), &[]));
    }

    #[test]
    fn test_request_matcher_path() {
        let matcher = RequestMatcher {
            method: "GET".to_string(),
            path: "/users".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
        };

        assert!(matcher.matches("GET", "/users", &HeaderMap::new(), &[]));
        assert!(matcher.matches("GET", "/users?id=1", &HeaderMap::new(), &[])); // Query ignored for path matching
        assert!(!matcher.matches("GET", "/posts", &HeaderMap::new(), &[]));
    }

    #[test]
    fn test_request_matcher_headers() {
        let mut matcher = RequestMatcher {
            method: "GET".to_string(),
            path: "/users".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
        };
        matcher.headers.insert("Authorization".to_string(), "Bearer token".to_string());

        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer token"));

        assert!(matcher.matches("GET", "/users", &headers, &[]));

        let empty_headers = HeaderMap::new();
        assert!(!matcher.matches("GET", "/users", &empty_headers, &[]));
    }

    #[test]
    fn test_request_matcher_query_params() {
        let mut matcher = RequestMatcher {
            method: "GET".to_string(),
            path: "/users".to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
        };
        matcher.query_params.insert("id".to_string(), "eq.1".to_string());

        let query = vec![("id".to_string(), "eq.1".to_string())];
        assert!(matcher.matches("GET", "/users", &HeaderMap::new(), &query));

        let wrong_query = vec![("id".to_string(), "eq.2".to_string())];
        assert!(!matcher.matches("GET", "/users", &HeaderMap::new(), &wrong_query));

        let no_query: Vec<(String, String)> = vec![];
        assert!(!matcher.matches("GET", "/users", &HeaderMap::new(), &no_query));
    }

    #[tokio::test]
    async fn test_mock_client_http_trait() {
        let mut mock = MockClient::new();
        mock.mock("GET", "/test").respond_with(200, "success");

        let response = mock.request(Method::GET, "/test")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_mock_client_no_match_returns_404() {
        let mock = MockClient::new();

        let response = mock.request(Method::GET, "/nonexistent")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_mock_client_with_query_params() {
        let mut mock = MockClient::new();
        mock.mock("GET", "/users")
            .match_query("id", "eq.1")
            .respond_with(200, r#"[{"id": 1}]"#);

        // Build query manually for test
        let query_slice: &[(&str, &str)] = &[("id", "eq.1")];

        let response = mock
            .request(Method::GET, "/users")
            .query(query_slice)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.text().await.unwrap();
        assert!(body.contains("\"id\""));
    }

    #[test]
    fn test_mock_json_helper() {
        let (status, body, headers) = mock_json(200, r#"{"test": true}"#);
        assert_eq!(status, 200);
        assert_eq!(body, r#"{"test": true}"#);
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
    }

    #[test]
    fn test_mock_error_helper() {
        let (status, body, _headers) = mock_error(500, "Server error");
        assert_eq!(status, 500);
        assert!(body.contains("Server error"));
    }
}
