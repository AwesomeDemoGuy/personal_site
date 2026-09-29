use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use std::{sync::Arc, time::Duration};

/// Shared HTTP client and refresh controls for the public weather endpoint.
#[cfg(feature = "ssr")]
#[derive(Clone)]
pub struct WeatherService {
    client: reqwest::Client,
    ca_refresh: Arc<tokio::sync::Mutex<()>>,
    az_refresh: Arc<tokio::sync::Mutex<()>>,
}

#[cfg(feature = "ssr")]
impl WeatherService {
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(5))
                .build()?,
            ca_refresh: Arc::new(tokio::sync::Mutex::new(())),
            az_refresh: Arc::new(tokio::sync::Mutex::new(())),
        })
    }
}

#[cfg(feature = "ssr")]
#[derive(Clone, Copy)]
enum WeatherLocation {
    Ca,
    Az,
}

#[cfg(feature = "ssr")]
impl WeatherLocation {
    fn parse(value: &str) -> Result<Self, ServerFnError> {
        if value.eq_ignore_ascii_case("CA") {
            Ok(Self::Ca)
        } else if value.eq_ignore_ascii_case("AZ") {
            Ok(Self::Az)
        } else {
            Err(ServerFnError::new("invalid weather location"))
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Ca => "CA",
            Self::Az => "AZ",
        }
    }
}

/// Weather data returned to the About page's browser widget.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Weather {
    pub temp_f: f64,
    pub description: String,
    pub emoji: String,
    pub is_stale: bool,
}

#[server(GetWeather, "/api/weather")]
pub async fn get_weather(location: String) -> Result<Weather, ServerFnError> {
    let location = WeatherLocation::parse(&location)?;
    let key = location.key();

    let pool = use_context::<sqlx::SqlitePool>()
        .ok_or_else(|| ServerFnError::new("database pool not available"))?;
    let service = use_context::<WeatherService>()
        .ok_or_else(|| ServerFnError::new("weather service not available"))?;

    let cached = weather_from_cache(&pool, key).await;
    if let Some(entry) = &cached {
        if !entry.is_stale {
            return Ok(entry.clone());
        }
    }

    let coordinates = weather_coordinates(key)?;
    let refresh_lock = match location {
        WeatherLocation::Ca => &service.ca_refresh,
        WeatherLocation::Az => &service.az_refresh,
    };
    let _refresh = match refresh_lock.try_lock() {
        Ok(guard) => guard,
        Err(_) => return cached_weather_or_busy(cached),
    };

    // Another request may have populated the cache between the first read and
    // acquiring the lock. Only one request per location proceeds to the API.
    if let Some(entry) = weather_from_cache(&pool, key).await {
        if !entry.is_stale {
            return Ok(entry);
        }
    }

    let weather = match fetch_weather_from_api(&service.client, coordinates).await {
        Ok(weather) => weather,
        Err(error) => {
            eprintln!("[weather] refresh failed for {key}: {error}");
            return cached_weather_or_busy(cached);
        }
    };
    // Best-effort cache write; a failure here shouldn't fail the request.
    if let Err(e) = weather_to_cache(&pool, key, &weather).await {
        eprintln!("[weather] cache write failed for {key}: {e}");
    }
    Ok(weather)
}

#[cfg(feature = "ssr")]
fn weather_coordinates(key: &str) -> Result<(f64, f64), ServerFnError> {
    let read = |suffix: &str| -> Result<f64, ServerFnError> {
        let var = format!("WEATHER_{key}_{suffix}");
        std::env::var(&var)
            .map_err(|_| ServerFnError::new("weather location unavailable"))?
            .trim()
            .parse::<f64>()
            .map_err(|_| ServerFnError::new("invalid weather coordinates"))
    };
    let lat = read("LAT")?;
    let lon = read("LON")?;
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return Err(ServerFnError::new("invalid weather coordinates"));
    }
    Ok((lat, lon))
}

#[cfg(feature = "ssr")]
fn cached_weather_or_busy(cached: Option<Weather>) -> Result<Weather, ServerFnError> {
    cached
        .ok_or_else(|| ServerFnError::new("weather temporarily unavailable"))
}

/// Return a fresh (under 30 minutes) or usable stale (under six hours) entry.
#[cfg(feature = "ssr")]
async fn weather_from_cache(pool: &sqlx::SqlitePool, key: &str) -> Option<Weather> {
    let row = sqlx::query_as::<_, (f64, String, String, i64)>(
        "SELECT temp_f, description, emoji, \
                fetched_at > datetime('now', '-30 minutes') \
         FROM weather_cache \
         WHERE location = ?1 \
           AND fetched_at > datetime('now', '-6 hours')",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()??;

    Some(Weather {
        temp_f: row.0,
        description: row.1,
        emoji: row.2,
        is_stale: row.3 == 0,
    })
}

/// Upsert the freshly fetched weather into the cache with the current time.
#[cfg(feature = "ssr")]
async fn weather_to_cache(
    pool: &sqlx::SqlitePool,
    key: &str,
    w: &Weather,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO weather_cache (location, temp_f, description, emoji, fetched_at) \
         VALUES (?1, ?2, ?3, ?4, datetime('now')) \
         ON CONFLICT(location) DO UPDATE SET \
            temp_f = excluded.temp_f, \
            description = excluded.description, \
            emoji = excluded.emoji, \
            fetched_at = excluded.fetched_at",
    )
    .bind(key)
    .bind(w.temp_f)
    .bind(&w.description)
    .bind(&w.emoji)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch current weather from Open-Meteo for validated coordinates.
/// Coordinates live in the server environment (e.g. a .env file), keyed by an
/// uppercased location id: WEATHER_<LOCATION>_LAT / _LON. They are never sent
/// to or read by the browser — only this server function uses them.
#[cfg(feature = "ssr")]
async fn fetch_weather_from_api(
    client: &reqwest::Client,
    (lat, lon): (f64, f64),
) -> Result<Weather, ServerFnError> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast\
         ?latitude={lat}&longitude={lon}\
         &current=temperature_2m,weather_code\
         &temperature_unit=fahrenheit"
    );

    let response: OpenMeteoResponse = client
        .get(&url)
        .send()
        .await
        .map_err(|_| ServerFnError::new("weather service unavailable"))?
        .error_for_status()
        .map_err(|_| ServerFnError::new("weather service unavailable"))?
        .json()
        .await
        .map_err(|_| ServerFnError::new("invalid weather response"))?;
    let temp_f = response.current.temperature_2m;
    let weather_code = response.current.weather_code;
    if !temp_f.is_finite() {
        return Err(ServerFnError::new("invalid weather response"));
    }

    Ok(Weather {
        temp_f,
        description: wmo_description(weather_code).to_string(),
        emoji: wmo_emoji(weather_code).to_string(),
        is_stale: false,
    })
}

#[cfg(feature = "ssr")]
#[derive(Deserialize)]
struct OpenMeteoResponse {
    current: OpenMeteoCurrent,
}

#[cfg(feature = "ssr")]
#[derive(Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f64,
    weather_code: i64,
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn only_displayed_locations_are_accepted() {
        assert!(matches!(WeatherLocation::parse("ca"), Ok(WeatherLocation::Ca)));
        assert!(matches!(WeatherLocation::parse("AZ"), Ok(WeatherLocation::Az)));
        for invalid in ["", "NV", "CA-WEST", "CA NORTH"] {
            assert!(WeatherLocation::parse(invalid).is_err(), "accepted {invalid:?}");
        }
    }

    #[test]
    fn cache_marks_stale_entries_and_expires_old_entries() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let pool = sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect("sqlite::memory:")
                    .await
                    .unwrap();
                sqlx::query(
                    "CREATE TABLE weather_cache (\
                     location TEXT PRIMARY KEY, temp_f REAL NOT NULL, \
                     description TEXT NOT NULL, emoji TEXT NOT NULL, fetched_at TEXT NOT NULL)",
                )
                .execute(&pool)
                .await
                .unwrap();
                for (location, age) in [
                    ("FRESH", "-5 minutes"),
                    ("STALE", "-1 hour"),
                    ("EXPIRED", "-7 hours"),
                ] {
                    sqlx::query(
                        "INSERT INTO weather_cache VALUES (?1, 72, 'Clear sky', 'sun', datetime('now', ?2))",
                    )
                    .bind(location)
                    .bind(age)
                    .execute(&pool)
                    .await
                    .unwrap();
                }

                assert!(!weather_from_cache(&pool, "FRESH").await.unwrap().is_stale);
                assert!(weather_from_cache(&pool, "STALE").await.unwrap().is_stale);
                assert!(weather_from_cache(&pool, "EXPIRED").await.is_none());
            });
    }
}

#[cfg(feature = "ssr")]
fn wmo_description(code: i64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Foggy",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 | 63 | 65 => "Rain",
        66 | 67 => "Freezing rain",
        71 | 73 | 75 => "Snow fall",
        77 => "Snow grains",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown weather code",
    }
}

// Emoji matching the WMO weather code, grouped the same way as wmo_description.
#[cfg(feature = "ssr")]
fn wmo_emoji(code: i64) -> &'static str {
    match code {
        0 => "☀️",                 // Clear sky
        1 => "🌤️",                 // Mainly clear
        2 => "⛅",                  // Partly cloudy
        3 => "☁️",                  // Overcast
        45 | 48 => "🌫️",           // Fog
        51 | 53 | 55 => "🌦️",      // Drizzle
        56 | 57 => "🌧️",           // Freezing drizzle
        61 | 63 | 65 => "🌧️",      // Rain
        66 | 67 => "🌧️",           // Freezing rain
        71 | 73 | 75 => "❄️",       // Snow fall
        77 => "🌨️",                // Snow grains
        80 | 81 | 82 => "🌧️",      // Rain showers
        85 | 86 => "🌨️",           // Snow showers
        95 => "⛈️",                 // Thunderstorm
        96 | 99 => "⛈️",            // Thunderstorm with hail
        _ => "❓",                  // Unknown
    }
}

#[component]
fn WeatherWidget(#[prop(into)] location: String) -> impl IntoView {
    let location_for_view = location.clone();
    let weather = Resource::new(
        || (),
        move |_| get_weather(location.clone()),
    );

    view! {
        <div class="weather-widget">
            <Suspense fallback=|| view! { <span class="weather-loading">"Loading weather…"</span> }>
                {move || weather.get().map(|res| match res {
                    Ok(w) => {
                        let cache_label = if w.is_stale { " (cached)" } else { "" };
                        view! {
                            <span>{format!("{} | {} {:.0}°F · {}{cache_label}", location_for_view.clone(), w.emoji, w.temp_f, w.description)}</span>
                        }.into_any()
                    },
                    Err(_) => view! { <span>"Weather unavailable"</span> }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[component]
pub fn AboutPage() -> impl IntoView {
    view! {
        <section class="page about">
            <WeatherWidget location="CA"/>
            <WeatherWidget location="AZ"/>

            <p class="about-email">
                // Split at the square brackets so the address can wrap around
                // the photo at those points (each piece is an atomic chip; the
                // container packs them with no gap, so it reads continuously).
                <span>"sebastian11ryan"</span>
                <span>"[at]"</span>
                <span>"gmail"</span>
                <span>"[dot]"</span>
                <span>"com -\u{00A0}"</span>
                <a href="/gpg">"GPG Key"</a>
            </p>

            <p class="intro">
                "Hi! I'm Sebastian Ashkar, a senior computer science undergrad at \
                Arizona State University. I have a particular interest in cyber \
                security."
            </p>

            <div class="links">
                <a
                    class="ext-link"
                    href="https://www.linkedin.com/in/sebastianashkar/"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    <img class="link-icon" src="/assets/LinkedIn.png" alt=""/>
                    "LinkedIn"
                </a>
                <a
                    class="ext-link"
                    href="https://github.com/AwesomeDemoGuy"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    <img class="link-icon" src="/assets/GitHub.svg" alt=""/>
                    "GitHub"
                </a>
                <a
                    class="ext-link"
                    href="/assets/Sebastian_Ashkar_Resume.pdf"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    <img class="link-icon" src="/assets/resume_icon.png" alt=""/>
                    "Resume"
                </a>
            </div>

            <CertificatesSection/>
            <TechnologiesSection/>
        </section>
    }
}

#[component]
fn CertificatesSection() -> impl IntoView {
    // Static list of certificates. Fields: (name, optional icon path, optional
    // link URL).
    let certificates: [(&str, Option<&str>, Option<&str>); 1] = [(
        "pwn.college Yellow Belt",
        Some("/assets/yellow_belt.svg"),
        Some("https://pwn.college/hacker/92956"),
    )];

    view! {
        <div class="section certificates">
            <h2>"Certificates"</h2>
            <div class="cert-cards">
                {certificates
                    .into_iter()
                    .map(|(name, icon, url)| {
                        // The icon starts inside the card's link but becomes a
                        // free, page-level draggable once moved (detaches to
                        // <body> and stops acting as a hyperlink). Browser-only.
                        let icon_view = icon.map(|src| {
                            let icon_ref = NodeRef::<leptos::html::Img>::new();
                            #[cfg(feature = "hydrate")]
                            {
                                use leptos::wasm_bindgen::JsCast;
                                icon_ref.on_load(move |el| {
                                    let element: web_sys::HtmlElement =
                                        el.unchecked_into();
                                    crate::interop::make_floating_draggable(&element);
                                });
                            }
                            view! {
                                <img
                                    node_ref=icon_ref
                                    class="cert-icon"
                                    src=src
                                    alt=""
                                    draggable="false"
                                />
                            }
                        });
                        match url {
                            Some(href) => view! {
                                <a
                                    class="cert-card"
                                    href=href
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    draggable="false"
                                >
                                    {icon_view}
                                    <span class="cert-name">{name}</span>
                                </a>
                            }.into_any(),
                            None => view! {
                                <div class="cert-card">
                                    {icon_view}
                                    <span class="cert-name">{name}</span>
                                </div>
                            }.into_any(),
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

#[component]
fn TechnologiesSection() -> impl IntoView {
    // Static list of technology / skill tags.
    let technologies = [
        "Python",
        "PostgreSQL",
        "Docker",
        "Return Oriented Programing",
        "IDA Pro",
        "c/c++",
        "Linux",
        "Cross Site Scripting",
        "SQL Injection",
        "x86 Assembly",
    ];

    view! {
        <div class="section technologies">
            <h2>"Technologies"</h2>
            <div class="tech-tags">
                {technologies
                    .into_iter()
                    .map(|t| view! { <span class="tech-tag">{t}</span> })
                    .collect_view()}
            </div>
        </div>
    }
}
