use serde::{Deserialize, Serialize};

/// A blog post. The body remains Markdown in SQLite and is converted on the server.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct BlogPost {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub body: String,
    /// ISO-8601 timestamp string. Kept as a string so the type is identical on
    /// the client (hydrate) and server (ssr) without pulling chrono into wasm.
    pub published_at: String,
}

/// The index does not need to transfer every post's body.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct BlogPostSummary {
    pub title: String,
    pub slug: String,
    pub published_at: String,
}

impl BlogPostSummary {
    pub fn url(&self) -> String {
        let mut url = String::from("/blog/");
        for byte in self.slug.bytes() {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                url.push(byte as char);
            } else {
                use std::fmt::Write;
                write!(url, "%{byte:02X}").expect("writing to a String cannot fail");
            }
        }
        url
    }
}

/// Only sanitized HTML leaves the server; the Markdown parser is absent from WASM.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderedBlogPost {
    pub title: String,
    pub html: String,
}

/// A portfolio project entry.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// Optional link to a repo or live site.
    pub url: Option<String>,
}
