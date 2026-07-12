//! # postgrest-rs
//!
//! [PostgREST][postgrest] client-side library.
//!
//! This library is a thin wrapper that brings an ORM-like interface to
//! PostgREST.
//!
//! ## Usage
//!
//! Simple example:
//! ```
//! use postgrest::Postgrest;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let client = Postgrest::new("https://your.postgrest.endpoint");
//! let resp = client
//!     .from("your_table")
//!     .select("*")
//!     .execute()
//!     .await?;
//! let body = resp
//!     .text()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! Using filters:
//! ```
//! # use postgrest::Postgrest;
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = Postgrest::new("https://your.postgrest.endpoint");
//! let resp = client
//!     .from("countries")
//!     .eq("name", "Germany")
//!     .gte("id", "20")
//!     .select("*")
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! Updating a table:
//! ```
//! # use postgrest::Postgrest;
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = Postgrest::new("https://your.postgrest.endpoint");
//! let resp = client
//!     .from("users")
//!     .eq("username", "soedirgo")
//!     .update("{\"organization\": \"supabase\"}")
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! Executing stored procedures:
//! ```
//! # use postgrest::Postgrest;
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! # let client = Postgrest::new("https://your.postgrest.endpoint");
//! let resp = client
//!     .rpc("add", r#"{"a": 1, "b": 2}"#)
//!     .execute()
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! Check out the [README][readme] for more info.
//!
//! [postgrest]: https://postgrest.org
//! [readme]: https://github.com/supabase/postgrest-rs

mod builder;
mod filter;
pub mod http;

#[cfg(feature = "mock")]
pub mod mock;

pub use builder::Builder;
use http::HttpClient;
use reqwest::header::{HeaderMap, HeaderValue, IntoHeaderName};
use reqwest::Client;

#[derive(Clone, Debug)]
pub struct PostgrestGeneric<C: HttpClient = Client> {
    url: String,
    schema: Option<String>,
    headers: HeaderMap,
    client: C,
}

pub type Postgrest = PostgrestGeneric<Client>;

impl Postgrest {
    /// Create with default reqwest client (backward compatible)
    /// Creates a Postgrest client.
    ///
    /// # Example
    ///
    /// ```
    /// use postgrest::Postgrest;
    ///
    /// let client = Postgrest::new("http://your.postgrest.endpoint");
    /// ```
    pub fn new<T: Into<String>>(url: T) -> Self {
        Self::with_client(url, reqwest::Client::new())
    }
}

impl<C: HttpClient> PostgrestGeneric<C> {
    pub fn with_client<T: Into<String>>(url: T, client: C) -> Self {
        Self {
            url: url.into(),
            schema: None,
            headers: HeaderMap::new(),
            client,
        }
    }

    /// Borrow the underlying HTTP client, for issuing requests to sibling
    /// Supabase services (Storage, Auth) that live outside the PostgREST base
    /// path and so cannot go through [`from`](Self::from) / [`rpc`](Self::rpc).
    pub fn http_client(&self) -> &C {
        &self.client
    }

    /// Switches the schema.
    ///
    /// # Note
    ///
    /// You can only switch schemas before you call `from` or `rpc`.
    ///
    /// # Example
    ///
    /// ```
    /// use postgrest::Postgrest;
    ///
    /// let client = Postgrest::new("http://your.postgrest.endpoint");
    /// client.schema("private");
    /// ```
    pub fn schema<T>(mut self, schema: T) -> Self
    where
        T: Into<String>,
    {
        self.schema = Some(schema.into());
        self
    }

    /// Add arbitrary headers to the request. For instance when you may want to connect
    /// through an API gateway that needs an API key header.
    ///
    /// # Example
    ///
    /// ```
    /// use postgrest::Postgrest;
    ///
    /// let client = Postgrest::new("https://your.postgrest.endpoint")
    ///     .insert_header("apikey", "super.secret.key")
    ///     .from("table");
    /// ```
    pub fn insert_header(
        mut self,
        header_name: impl IntoHeaderName,
        header_value: impl AsRef<str>,
    ) -> Self {
        self.headers.insert(
            header_name,
            HeaderValue::from_str(header_value.as_ref()).expect("Invalid header value."),
        );
        self
    }

    /// Perform a table operation.
    ///
    /// # Example
    ///
    /// ```
    /// use postgrest::Postgrest;
    ///
    /// let client = Postgrest::new("http://your.postgrest.endpoint");
    /// client.from("table");
    /// ```
    pub fn from<T>(&self, table: T) -> Builder<C>
    where
        T: AsRef<str>,
    {
        let url = format!("{}/{}", self.url, table.as_ref());
        Builder::new(
            url,
            self.schema.clone(),
            self.headers.clone(),
            self.client.clone(),
        )
    }

    /// Perform a stored procedure call.
    ///
    /// # Example
    ///
    /// ```
    /// use postgrest::Postgrest;
    ///
    /// let client = Postgrest::new("http://your.postgrest.endpoint");
    /// client.rpc("multiply", r#"{"a": 1, "b": 2}"#);
    /// ```
    pub fn rpc<T, U>(&self, function: T, params: U) -> Builder<C>
    where
        T: AsRef<str>,
        U: Into<String>,
    {
        let url = format!("{}/rpc/{}", self.url, function.as_ref());
        Builder::new(
            url,
            self.schema.clone(),
            self.headers.clone(),
            self.client.clone(),
        )
        .rpc(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REST_URL: &str = "http://localhost:3000";

    #[test]
    fn initialize() {
        assert_eq!(Postgrest::new(REST_URL).url, REST_URL);
    }

    #[test]
    fn switch_schema() {
        assert_eq!(
            Postgrest::new(REST_URL).schema("private").schema,
            Some("private".to_string())
        );
    }

    #[test]
    fn with_insert_header() {
        assert_eq!(
            Postgrest::new(REST_URL)
                .insert_header("apikey", "super.secret.key")
                .headers
                .get("apikey")
                .unwrap(),
            "super.secret.key"
        );
    }
}
