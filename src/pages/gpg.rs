use leptos::prelude::*;

/// The ASCII-armored public key, served verbatim to non-browser clients (curl,
/// wget, …) that request `/gpg` — see the content-negotiation middleware in
/// `main.rs`. Browsers get the flowing HTML presentation below.
pub const PUBLIC_KEY: &str = r#"-----BEGIN PGP PUBLIC KEY BLOCK-----

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
p16hHT6ZbgKMfwGPfoyiM+Uh8ZNMUxM=
=j8ep
-----END PGP PUBLIC KEY BLOCK-----"#;

#[component]
pub fn GpgPage() -> impl IntoView {
    // Browser presentation: the armor `-----BEGIN-----`/`-----END-----` lines
    // and the CRC checksum line (leading '=') each stay on their own rows,
    // separated from the flowing base64 body by a blank line, so it reads like a
    // proper key block. The body itself is collapsed to a single newline-free
    // line that the text-flow layout wraps around the photo. Non-browser clients
    // (curl, wget) get the valid multi-line key via the middleware in main.rs.
    let flowing_key = {
        let lines: Vec<&str> = PUBLIC_KEY.lines().collect();
        match lines.as_slice() {
            [begin, middle @ .., end] => match middle.split_last() {
                // Checksum present: body\n=checksum\n\nEND — the checksum sits
                // directly under the body (one newline), with a blank line
                // between it and the footer.
                Some((checksum, body)) if checksum.starts_with('=') => {
                    format!("{begin}\n\n{}\n{checksum}\n\n{end}", body.concat())
                }
                _ => format!("{begin}\n\n{}\n\n{end}", middle.concat()),
            },
            _ => PUBLIC_KEY.to_string(),
        }
    };

    view! {
        <section class="page gpg">
            <h1>"GPG Key"</h1>
            <pre>{flowing_key}</pre>
        </section>
    }
}
