//! Priorite basse #8 strategy doc §9 : sanitize serveur du Profil README.
//!
//! Le rendu Markdown safe se fait cote frontend (sanitize-html + safelist).
//! Ce module implemente la **defense en profondeur** : si un endpoint sans
//! sanitizer frontend ecrit un jour dans `users.profile_readme_markdown`
//! (ex: import GitHub via profile_readme_sync, futur admin bulk-import, etc.),
//! on s'assure que les vecteurs XSS/phishing les plus evidents sont neutralises
//! en amont.
//!
//! Ce sanitizer travaille sur du **markdown source** — pas du HTML rendu. Il
//! neutralise ce qui n'est pas markdown standard :
//! - Blocs `<script>...</script>` et `<style>...</style>` (contenu drop)
//! - Attributs `on*=` sur toute balise HTML inline (drop tag entier)
//! - URLs `javascript:` et `data:text/html` dans href/src (drop tag entier)
//! - Iframes non-safelistees (drop tag entier)
//! - Formulaires HTML (drop tag entier)
//!
//! Ce qu'on GARDE (permissible par strategy doc):
//! - Markdown pur (headers, listes, tables, quotes, links, images)
//! - Emojis Unicode
//! - `<img>` tags avec src `https?://...` (safelist domaines fait cote FE)
//! - Iframes safelistees (youtube.com/embed, player.vimeo.com, giphy.com)
//! - Badges `img.shields.io/...`

const IFRAME_SAFELIST_DOMAINS: &[&str] = &[
    "youtube.com/embed/",
    "www.youtube.com/embed/",
    "player.vimeo.com/video/",
    "giphy.com/embed/",
];

/// Nettoie un README markdown source avant persistance.
/// Idempotent : sanitize(sanitize(x)) == sanitize(x).
pub fn sanitize_readme_markdown(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    let bytes = input.as_bytes();

    while cursor < bytes.len() {
        // Detecte les blocs "code fence" markdown ```...``` — le contenu est
        // preserve integralement (c'est du texte, pas du HTML rendu).
        if bytes[cursor..].starts_with(b"```") {
            let start = cursor;
            cursor += 3;
            // Trouve la fin
            while cursor + 3 <= bytes.len() && !bytes[cursor..].starts_with(b"```") {
                cursor += 1;
            }
            cursor = (cursor + 3).min(bytes.len());
            output.push_str(&input[start..cursor]);
            continue;
        }

        // Balise ouvrante potentielle
        if bytes[cursor] == b'<' {
            if let Some(tag_end_off) = find_tag_end(&bytes[cursor..]) {
                let tag = &input[cursor..cursor + tag_end_off + 1];
                if is_dangerous_tag(tag) {
                    // Skippe le tag ET son contenu si c'est un tag "container"
                    // dangereux (script/style/iframe non-safelist/form).
                    let lower_tag_name = extract_tag_name(tag).to_lowercase();
                    if matches!(
                        lower_tag_name.as_str(),
                        "script" | "style" | "form" | "iframe"
                    ) {
                        // Skippe jusqu'a la balise fermante
                        let close_pattern = format!("</{lower_tag_name}");
                        let search_from = cursor + tag_end_off + 1;
                        if let Some(close_pos) =
                            find_pattern_case_insensitive(&input[search_from..], &close_pattern)
                        {
                            let after_close = search_from + close_pos;
                            // Skippe jusqu'au '>' de la fermante
                            let mut end = after_close;
                            while end < bytes.len() && bytes[end] != b'>' {
                                end += 1;
                            }
                            cursor = (end + 1).min(bytes.len());
                            continue;
                        }
                        // Si pas de fermante trouvee, on drop le reste
                        cursor = bytes.len();
                        continue;
                    }
                    // Tag inline dangereux (a/img avec javascript: etc) — on
                    // le drop mais on garde le reste apres.
                    cursor += tag_end_off + 1;
                    continue;
                }
                // Tag safe — copie-le
                output.push_str(tag);
                cursor += tag_end_off + 1;
                continue;
            }
        }

        // Caractere ordinaire
        output.push(bytes[cursor] as char);
        cursor += 1;
    }

    output
}

fn find_tag_end(bytes: &[u8]) -> Option<usize> {
    for (i, &b) in bytes.iter().enumerate().skip(1) {
        if b == b'>' {
            return Some(i);
        }
    }
    None
}

fn extract_tag_name(tag: &str) -> String {
    let trimmed = tag
        .trim_start_matches('<')
        .trim_start_matches('/')
        .trim_end_matches('>');
    trimmed
        .split(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .next()
        .unwrap_or("")
        .to_string()
}

fn find_pattern_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack_lower = haystack.to_lowercase();
    let needle_lower = needle.to_lowercase();
    haystack_lower.find(&needle_lower)
}

fn is_dangerous_tag(tag: &str) -> bool {
    let lower = tag.to_lowercase();

    // Attributs on* (event handlers)
    // Simple detection : " on{alpha}=" pattern
    let bytes = lower.as_bytes();
    for (i, w) in bytes.windows(3).enumerate() {
        if w == b" on" && i + 3 < bytes.len() {
            let next = bytes[i + 3];
            if next.is_ascii_alphabetic() {
                return true;
            }
        }
    }

    // URLs dangereuses
    if lower.contains("javascript:") || lower.contains("data:text/html") {
        return true;
    }

    // Tags conteneurs dangereux (le sanitizer traitera le contenu)
    let name = extract_tag_name(tag).to_lowercase();
    if matches!(name.as_str(), "script" | "style" | "form") {
        return true;
    }

    // Iframe : safelist stricte
    if name == "iframe" {
        let has_safelisted_src = IFRAME_SAFELIST_DOMAINS.iter().any(|domain| {
            lower.contains(&format!("src=\"https://{domain}"))
                || lower.contains(&format!("src=\"http://{domain}"))
                || lower.contains(&format!("src='https://{domain}"))
                || lower.contains(&format!("src='http://{domain}"))
        });
        if !has_safelisted_src {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_script_tag_and_content() {
        let input = r#"Hello <script>alert('xss')</script>world"#;
        let out = sanitize_readme_markdown(input);
        assert!(!out.contains("<script"));
        assert!(!out.contains("alert"));
        assert!(out.contains("Hello"));
        assert!(out.contains("world"));
    }

    #[test]
    fn drops_style_tag_and_content() {
        let input = r#"a <style>body{display:none}</style> b"#;
        let out = sanitize_readme_markdown(input);
        assert!(!out.contains("<style"));
        assert!(!out.contains("display:none"));
    }

    #[test]
    fn drops_onload_handler() {
        let input = r#"<img src="a.png" onload="alert(1)">"#;
        let out = sanitize_readme_markdown(input);
        assert!(!out.contains("onload"));
        assert!(!out.contains("alert"));
    }

    #[test]
    fn drops_javascript_url() {
        let input = r#"<a href="javascript:alert(1)">click</a>"#;
        let out = sanitize_readme_markdown(input);
        assert!(!out.contains("javascript:"));
    }

    #[test]
    fn drops_data_text_html_url() {
        let input = r#"<a href="data:text/html,<script>alert(1)</script>">x</a>"#;
        let out = sanitize_readme_markdown(input);
        assert!(!out.contains("data:text/html"));
    }

    #[test]
    fn drops_non_safelisted_iframe() {
        let input = r#"<iframe src="https://evil.example.com/xss"></iframe>"#;
        let out = sanitize_readme_markdown(input);
        assert!(!out.contains("<iframe"));
        assert!(!out.contains("evil.example.com"));
    }

    #[test]
    fn keeps_safelisted_youtube_iframe() {
        let input = r#"<iframe src="https://www.youtube.com/embed/dQw4w9WgXcQ"></iframe>"#;
        let out = sanitize_readme_markdown(input);
        assert!(out.contains("youtube.com/embed"));
    }

    #[test]
    fn drops_form_tag() {
        let input = r#"<form action="evil.com"><input name="p"></form>"#;
        let out = sanitize_readme_markdown(input);
        assert!(!out.contains("<form"));
        assert!(!out.contains("<input"));
    }

    #[test]
    fn preserves_pure_markdown() {
        let input = "# Titre\n\nUn paragraphe [avec lien](https://example.com) et **gras**.\n\n- item 1\n- item 2\n";
        let out = sanitize_readme_markdown(input);
        assert_eq!(out, input);
    }

    #[test]
    fn preserves_code_fence_with_dangerous_content() {
        // Le contenu dans un code fence est du TEXTE, pas du HTML — on le garde.
        let input = "```html\n<script>alert(1)</script>\n```\n";
        let out = sanitize_readme_markdown(input);
        assert!(out.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn preserves_img_tag_safe() {
        let input = r#"<img src="https://img.shields.io/badge/skilluv-forge-orange">"#;
        let out = sanitize_readme_markdown(input);
        assert!(out.contains("img.shields.io"));
    }

    #[test]
    fn idempotent() {
        let input = r#"Hello <script>bad</script> <img src="ok.png" onload="bad()"> world"#;
        let out1 = sanitize_readme_markdown(input);
        let out2 = sanitize_readme_markdown(&out1);
        assert_eq!(out1, out2);
    }
}
