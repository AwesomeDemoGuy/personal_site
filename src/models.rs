use serde::{Deserialize, Serialize};

/// A blog post. Body is stored as raw text/markdown; rendering is handled in
/// the UI layer (to be filled in later).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlogPost {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub body: String,
    /// ISO-8601 timestamp string. Kept as a string so the type is identical on
    /// the client (hydrate) and server (ssr) without pulling chrono into wasm.
    pub published_at: String,
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

/// A certificate / credential shown on the About page.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Certificate {
    pub id: i64,
    pub name: String,
    pub issuer: String,
    pub year: i64,
}

/// A technology / skill tag shown on the About page.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Technology {
    pub id: i64,
    pub name: String,
    /// Free-form grouping, e.g. "Languages", "Frameworks", "Tools".
    pub category: String,
}
