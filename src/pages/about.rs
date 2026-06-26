use leptos::prelude::*;
use serde::{Deserialize, Serialize};


/// The default landing page. Holds the intro, external links, certificates,
/// and technologies.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Weather {
    pub temp_f: f64,
    pub description: String,
    pub emoji: String,
}

#[server(GetWeather, "/api/weather")]
pub async fn get_weather(location: String) -> Result<Weather, ServerFnError> {
    let key = location.to_uppercase();

    // Serve from the SQLite cache if we fetched this location within the last
    // 30 minutes; otherwise fetch fresh from the API and update the cache.
    let pool = use_context::<sqlx::SqlitePool>()
        .ok_or_else(|| ServerFnError::new("database pool not available"))?;

    if let Some(cached) = weather_from_cache(&pool, &key).await {
        return Ok(cached);
    }
    let weather = fetch_weather_from_api(&key).await?;
    // Best-effort cache write; a failure here shouldn't fail the request.
    if let Err(e) = weather_to_cache(&pool, &key, &weather).await {
        eprintln!("[weather] cache write failed for {key}: {e}");
    }
    Ok(weather)
}

/// Return the cached weather for `key` if present and fetched within the TTL
/// (30 minutes). Returns `None` on miss, staleness, or any DB error.
#[cfg(feature = "ssr")]
async fn weather_from_cache(pool: &sqlx::SqlitePool, key: &str) -> Option<Weather> {
    // `fetched_at` is stored as a UTC datetime string; compare against now-30m.
    let row = sqlx::query_as::<_, (f64, String, String)>(
        "SELECT temp_f, description, emoji \
         FROM weather_cache \
         WHERE location = ?1 \
           AND fetched_at > datetime('now', '-30 minutes')",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .ok()??;

    Some(Weather {
        temp_f: row.0,
        description: row.1,
        emoji: row.2,
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

/// Fetch current weather from the Open-Meteo API for the given location key.
/// Coordinates live in the server environment (e.g. a .env file), keyed by an
/// uppercased location id: WEATHER_<LOCATION>_LAT / _LON. They are never sent
/// to or read by the browser — only this server function uses them.
#[cfg(feature = "ssr")]
async fn fetch_weather_from_api(key: &str) -> Result<Weather, ServerFnError> {
    let read = |suffix: &str| -> Result<f64, ServerFnError> {
        let var = format!("WEATHER_{key}_{suffix}");
        std::env::var(&var)
            .map_err(|_| ServerFnError::new(format!("missing env var {var}")))?
            .trim()
            .parse::<f64>()
            .map_err(|_| ServerFnError::new(format!("invalid number in env var {var}")))
    };
    let lat = read("LAT")?;
    let lon = read("LON")?;

    let url = format!(
        "https://api.open-meteo.com/v1/forecast\
         ?latitude={lat}&longitude={lon}\
         &current=temperature_2m,weather_code\
         &temperature_unit=fahrenheit"
    );

    let resp: serde_json::Value = reqwest::get(&url)
        .await
        .map_err(|e| ServerFnError::new(format!("weather request failed: {e}")))?
        .json()
        .await
        .map_err(|e| ServerFnError::new(format!("weather parse failed: {e}")))?;

    let current = &resp["current"];
    let temp_f = current["temperature_2m"].as_f64().unwrap_or(0.0);
    let weather_code = current["weather_code"].as_i64().unwrap_or(0);

    Ok(Weather {
        temp_f,
        description: wmo_description(weather_code).to_string(),
        emoji: wmo_emoji(weather_code).to_string(),
    })
}

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
                    Ok(w) => view! {
                        <span>{format!("{} | {} {:.0}°F · {}", location_for_view.clone(), w.emoji, w.temp_f, w.description)}</span>
                    }.into_any(),
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
            <h1>"About Me"</h1>

            <WeatherWidget location="CA"/>
            <WeatherWidget location="AZ"/>

            <p class="intro">
                // Placeholder bio — replaced with real copy later. Long enough
                // that dragging the photo over it visibly displaces the text.
                "Short introduction goes here. This paragraph is intentionally a \
                 few sentences long so that the pretext layout engine has real \
                 text to flow around the profile photo. Try grabbing the photo \
                 above and dragging it down across these lines: the text reflows \
                 to make room for it, shifting to whichever side has space and \
                 falling back to full width once the photo no longer overlaps a \
                 given line. None of this touches the browser's own layout for \
                 measurement — pretext computes the line breaks from cached glyph \
                 widths, which keeps the reflow smooth while you drag."
            </p>

            <div class="links">
                <a
                    class="ext-link"
                    href="https://www.linkedin.com/in/sebastianashkar/"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    "LinkedIn"
                </a>
                <a
                    class="ext-link"
                    href="https://github.com/AwesomeDemoGuy"
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    "GitHub"
                </a>
            </div>

            <CertificatesSection/>
            <TechnologiesSection/>
        </section>
    }
}

#[component]
fn CertificatesSection() -> impl IntoView {
    // Placeholder list; will be sourced from the database later.
    let placeholders = ["Yellow Belt", "Certificate two", "Certificate three"];

    view! {
        <div class="section certificates">
            <h2>"Certificates"</h2>
            <ul class="cert-list">
                {placeholders
                    .into_iter()
                    .map(|c| view! { <li class="cert-item">{c}</li> })
                    .collect_view()}
            </ul>
        </div>
    }
}

#[component]
fn TechnologiesSection() -> impl IntoView {
    // Placeholder tags; will be sourced from the database later.
    let placeholders = ["Python", "PostgreSQL", "Docker", "ROP", "IDA Pro"];

    view! {
        <div class="section technologies">
            <h2>"Technologies"</h2>
            <div class="tech-tags">
                {placeholders
                    .into_iter()
                    .map(|t| view! { <span class="tech-tag">{t}</span> })
                    .collect_view()}
            </div>
        </div>
    }
}
