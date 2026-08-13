//! The email skeleton. One, for every message the platform sends.
//!
//! ## Why one
//!
//! A template per email, times a language, is how a codebase ends up with two
//! hundred HTML files and a button that looks different in nine of them. The
//! text comes from the translation catalogue; this module owns the frame:
//! header, typography, colours, the call to action, the footer.
//!
//! Changing the brand is changing this file.
//!
//! ## Why it looks like 2005 HTML
//!
//! Tables, inline styles, no flexbox, no grid, no external stylesheet.
//! Not nostalgia — Outlook renders through Word, Gmail strips `<style>`
//! blocks in some clients and rewrites classes, and none of them agree on
//! anything from this century. Inline styles on nested tables is the only
//! layout every mail client renders the same way.
//!
//! ## Colours
//!
//! Taken from the frontend's Forge theme (`src/app.css`), so an email and
//! the app look like the same product. Emails use the light variant: a dark
//! background is fine in an app someone chose to open and hostile in an inbox
//! sitting between two white messages.

use crate::services::i18n;

/// Forge, light. Mirrors `[data-theme='forge-light']` in the frontend.
const SURFACE: &str = "#f4ede0"; // parchment
const CARD: &str = "#fffaf3";
const BORDER: &str = "#d9c9b0";
const TEXT: &str = "#2a211a";
const TEXT_MUTED: &str = "#6b5c4a";
const ACCENT: &str = "#c47a2e"; // ochre, the signature
const ACCENT_TEXT: &str = "#ffffff";
const LUV: &str = "#e63946";

/// Everything that varies between two emails.
pub struct Email<'a> {
    /// BCP-47 tag. Drives `lang`, and `dir` for right-to-left scripts.
    pub locale: &'a str,
    /// Already translated, already interpolated.
    pub title: &'a str,
    pub body: &'a str,
    pub recipient_name: Option<&'a str>,
    pub cta_label: Option<&'a str>,
    pub cta_url: Option<&'a str>,
    /// Omitted for transactional mail, which has no unsubscribe: a payout
    /// receipt is not marketing, and offering to opt out of it would be a
    /// promise we cannot keep.
    pub unsubscribe_url: Option<&'a str>,
}

/// Escape text destined for HTML.
///
/// A display name is whatever its owner typed. Interpolating it raw into a
/// message sent to someone else is how a stored injection reaches an inbox.
fn escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Render a complete HTML email.
pub fn render(email: Email<'_>) -> String {
    let dir = i18n::direction(email.locale);
    let lang = email.locale;
    // Right-to-left flips which side text hangs on. Getting this wrong is
    // immediately visible to an Arabic reader and invisible to everyone else,
    // which is exactly why it is handled here rather than left to each email.
    let align = if dir == "rtl" { "right" } else { "left" };

    let title = escape(email.title);
    let body = escape(email.body).replace('\n', "<br>");

    let greeting = match email.recipient_name {
        Some(name) => format!(
            r#"<p style="margin:0 0 16px;font-size:16px;color:{TEXT};">{}</p>"#,
            escape(&i18n::t_with(
                email.locale,
                "email.greeting",
                &[("name", name)]
            ))
        ),
        None => String::new(),
    };

    let cta = match (email.cta_label, email.cta_url) {
        (Some(label), Some(url)) => format!(
            r#"
              <table role="presentation" cellpadding="0" cellspacing="0" border="0" style="margin:28px 0;">
                <tr><td style="border-radius:8px;background:{ACCENT};">
                  <a href="{url}" style="display:inline-block;padding:14px 28px;font-family:Georgia,'Times New Roman',serif;font-size:16px;font-weight:bold;color:{ACCENT_TEXT};text-decoration:none;border-radius:8px;">{label}</a>
                </td></tr>
              </table>"#,
            url = escape(url),
            label = escape(label),
        ),
        _ => String::new(),
    };

    let unsubscribe = match email.unsubscribe_url {
        Some(url) => format!(
            r#"<p style="margin:12px 0 0;font-size:12px;color:{TEXT_MUTED};">
                 <a href="{url}" style="color:{TEXT_MUTED};text-decoration:underline;">{label}</a>
               </p>"#,
            url = escape(url),
            label = escape(&i18n::t(email.locale, "email.unsubscribe")),
        ),
        None => String::new(),
    };

    let footer_note = escape(&i18n::t(email.locale, "email.footer_note"));

    format!(
        r#"<!doctype html>
<html lang="{lang}" dir="{dir}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
</head>
<body style="margin:0;padding:0;background:{SURFACE};">
  <!-- Preheader: the grey line an inbox shows next to the subject. Left
       empty-looking on purpose so it does not repeat the subject. -->
  <div style="display:none;max-height:0;overflow:hidden;opacity:0;">{title}</div>

  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="background:{SURFACE};padding:32px 12px;">
    <tr>
      <td align="center">
        <table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="max-width:560px;background:{CARD};border:1px solid {BORDER};border-radius:12px;overflow:hidden;">

          <tr><td style="height:6px;background:{ACCENT};font-size:0;line-height:0;">&nbsp;</td></tr>

          <tr>
            <td style="padding:28px 32px 0;text-align:{align};">
              <span style="font-family:Georgia,'Times New Roman',serif;font-size:22px;font-weight:bold;color:{TEXT};letter-spacing:-0.5px;">skill<span style="color:{LUV};">uv</span></span>
            </td>
          </tr>

          <tr>
            <td style="padding:20px 32px 8px;text-align:{align};">
              <h1 style="margin:0;font-family:Georgia,'Times New Roman',serif;font-size:26px;line-height:1.25;font-weight:bold;color:{TEXT};">{title}</h1>
            </td>
          </tr>

          <tr>
            <td style="padding:12px 32px 0;text-align:{align};font-family:-apple-system,'Segoe UI',Arial,sans-serif;">
              {greeting}
              <div style="font-size:16px;line-height:1.6;color:{TEXT};">{body}</div>
              {cta}
            </td>
          </tr>

          <tr><td style="padding:0 32px;"><div style="height:1px;background:{BORDER};"></div></td></tr>

          <tr>
            <td style="padding:20px 32px 28px;text-align:{align};font-family:-apple-system,'Segoe UI',Arial,sans-serif;">
              <p style="margin:0;font-size:12px;line-height:1.5;color:{TEXT_MUTED};">{footer_note}</p>
              {unsubscribe}
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(locale: &str) -> Email<'_> {
        Email {
            locale,
            title: "Ton paiement est parti",
            body: "42,50 € ont été envoyés sur ton Mobile Money.",
            recipient_name: Some("Ada"),
            cta_label: Some("Voir le détail"),
            cta_url: Some("https://skill-uv.com/wallet"),
            unsubscribe_url: None,
        }
    }

    #[test]
    fn it_renders_a_complete_document() {
        let html = render(sample("fr"));
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Ton paiement est parti"));
        assert!(html.contains("https://skill-uv.com/wallet"));
        assert!(html.contains("lang=\"fr\""));
    }

    #[test]
    fn arabic_flips_the_document_direction() {
        let html = render(sample("ar"));
        assert!(html.contains(r#"dir="rtl""#));
        assert!(
            html.contains("text-align:right"),
            "an RTL email that stays left-aligned is visibly broken"
        );
    }

    #[test]
    fn latin_scripts_stay_left_to_right() {
        let html = render(sample("fr"));
        assert!(html.contains(r#"dir="ltr""#));
        assert!(html.contains("text-align:left"));
    }

    #[test]
    fn user_supplied_text_cannot_inject_markup() {
        let html = render(Email {
            locale: "fr",
            title: "<script>alert(1)</script>",
            body: "ok",
            recipient_name: Some("<img src=x onerror=alert(1)>"),
            cta_label: None,
            cta_url: None,
            unsubscribe_url: None,
        });
        assert!(!html.contains("<script>"), "markup must not survive");
        assert!(!html.contains("onerror="));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn a_url_is_escaped_too() {
        let html = render(Email {
            locale: "fr",
            title: "t",
            body: "b",
            recipient_name: None,
            cta_label: Some("Go"),
            cta_url: Some("https://x.test/\" onmouseover=\"alert(1)"),
            unsubscribe_url: None,
        });
        assert!(
            !html.contains("onmouseover=\"alert"),
            "an attribute must not be breakable from a URL"
        );
    }

    #[test]
    fn no_call_to_action_renders_no_button() {
        let html = render(Email {
            locale: "fr",
            title: "t",
            body: "b",
            recipient_name: None,
            cta_label: None,
            cta_url: None,
            unsubscribe_url: None,
        });
        assert!(!html.contains("border-radius:8px;background:"));
    }

    #[test]
    fn transactional_mail_offers_no_unsubscribe() {
        // Offering to opt out of a payout receipt is a promise we cannot
        // keep, so the link is absent rather than inert.
        let html = render(sample("fr"));
        assert!(!html.contains("unsubscribe"));
    }

    #[test]
    fn line_breaks_in_the_body_survive() {
        let html = render(Email {
            locale: "fr",
            title: "t",
            body: "ligne un\nligne deux",
            recipient_name: None,
            cta_label: None,
            cta_url: None,
            unsubscribe_url: None,
        });
        assert!(html.contains("ligne un<br>ligne deux"));
    }
}
