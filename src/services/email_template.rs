//! The email skeleton. One, in whichever world the reader chose.
//!
//! ## Why one skeleton
//!
//! A template per email, times a language, times five themes, would be a
//! thousand files and a button that looks different in nine of them. The
//! text comes from the translation catalogue, the colours from
//! [`email_theme`]; this module owns the frame.
//!
//! ## Why it is not corporate
//!
//! The frontend does not ship a colour scheme, it ships five worlds — the
//! smith's workshop, the lantern-lit night, the tournament, the copyist's
//! desk, the cherry blossom season. Someone chose one. A grey transactional
//! email afterwards reads like a different company wrote it.
//!
//! So the frame carries the world: its palette, a rule under the wordmark in
//! its accent, and its tagline. Not decoration — it is what tells a reader
//! at a glance that this came from the place they picked that world in.
//!
//! ## Why it looks like 2005 HTML
//!
//! Tables, inline styles, no flexbox, no external stylesheet. Not nostalgia:
//! Outlook renders through Word, Gmail strips `<style>` blocks in some
//! clients and rewrites classes, and none of them agree on anything from
//! this century. Inline styles on nested tables is the only layout every
//! mail client renders the same way.

use crate::services::email_theme::{self, Theme};
use crate::services::i18n;

/// Everything that varies between two emails.
pub struct Email<'a> {
    /// BCP-47 tag. Drives `lang`, and `dir` for right-to-left scripts.
    pub locale: &'a str,
    /// The reader's chosen world. `None` falls back to the workshop.
    pub theme: Option<&'a str>,
    /// Already translated, already interpolated.
    pub title: &'a str,
    pub body: &'a str,
    pub recipient_name: Option<&'a str>,
    /// Figures to show under the body, as (label, value) already translated.
    ///
    /// The digest is the reason this exists: it has a shape no sentence
    /// carries. Structured rather than a slab of HTML the caller builds,
    /// so the design stays here and a second theme does not mean a second
    /// digest.
    pub stats: &'a [(String, String)],
    /// The one thing to do next. Emails with no action have no button
    /// rather than a limp "open the app".
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
    let t: Theme = email_theme::resolve(email.theme);
    let dir = i18n::direction(email.locale);
    let lang = email.locale;
    // Right-to-left flips which side text hangs on. Getting this wrong is
    // immediately visible to an Arabic reader and invisible to everyone
    // else, which is why it is handled here rather than per email.
    let align = if dir == "rtl" { "right" } else { "left" };

    let title = escape(email.title);
    let body = escape(email.body).replace('\n', "<br>");

    let greeting = match email.recipient_name {
        Some(name) => format!(
            r#"<p style="margin:0 0 18px;font-size:16px;color:{muted};">{}</p>"#,
            escape(&i18n::t_with(
                email.locale,
                "email.greeting",
                &[("name", name)]
            )),
            muted = t.text_muted,
        ),
        None => String::new(),
    };

    // One row of figures. Laid out as a table rather than flex or grid:
    // Outlook renders neither, and a digest that collapses into a column of
    // orphaned numbers is worse than no digest.
    let stats = if email.stats.is_empty() {
        String::new()
    } else {
        let cells: String = email
            .stats
            .iter()
            .map(|(label, value)| {
                format!(
                    r#"<td align="center" style="padding:14px 10px;">
                         <div style="font-family:Georgia,'Times New Roman',serif;font-size:26px;font-weight:bold;color:{accent};">{value}</div>
                         <div style="font-size:12px;color:{muted};letter-spacing:0.3px;">{label}</div>
                       </td>"#,
                    value = escape(value),
                    label = escape(label),
                    accent = t.accent,
                    muted = t.text_muted,
                )
            })
            .collect();
        format!(
            r#"<table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0"
                      style="margin:22px 0;border-radius:10px;background:{surface};">
                 <tr>{cells}</tr>
               </table>"#,
            surface = t.surface,
        )
    };

    let cta = match (email.cta_label, email.cta_url) {
        (Some(label), Some(url)) => format!(
            r#"
              <table role="presentation" cellpadding="0" cellspacing="0" border="0" style="margin:30px 0 6px;">
                <tr><td style="border-radius:10px;background:{accent};">
                  <a href="{url}" style="display:inline-block;padding:15px 32px;font-family:Georgia,'Times New Roman',serif;font-size:16px;font-weight:bold;color:{accent_fg};text-decoration:none;border-radius:10px;letter-spacing:0.2px;">{label}</a>
                </td></tr>
              </table>"#,
            url = escape(url),
            label = escape(label),
            accent = t.accent,
            accent_fg = t.accent_fg,
        ),
        _ => String::new(),
    };

    let unsubscribe = match email.unsubscribe_url {
        Some(url) => format!(
            r#"<p style="margin:10px 0 0;font-size:12px;color:{muted};">
                 <a href="{url}" style="color:{muted};text-decoration:underline;">{label}</a>
               </p>"#,
            url = escape(url),
            label = escape(&i18n::t(email.locale, "email.unsubscribe")),
            muted = t.text_muted,
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
<body style="margin:0;padding:0;background:{surface};">
  <!-- Preheader: the grey line an inbox shows beside the subject. Kept
       invisible and distinct from the subject so it does not repeat it. -->
  <div style="display:none;max-height:0;overflow:hidden;opacity:0;">{footer_note}</div>

  <table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="background:{surface};padding:36px 12px;">
    <tr>
      <td align="center">
        <table role="presentation" width="100%" cellpadding="0" cellspacing="0" border="0" style="max-width:560px;background:{card};border:1px solid {border};border-radius:14px;overflow:hidden;">

          <!-- The world's colour, across the top. -->
          <tr><td style="height:5px;background:{accent};font-size:0;line-height:0;">&nbsp;</td></tr>

          <tr>
            <td style="padding:30px 34px 0;text-align:{align};">
              <span style="font-family:Georgia,'Times New Roman',serif;font-size:21px;font-weight:bold;color:{text};letter-spacing:-0.4px;">skill<span style="color:{accent};">uv</span></span>
              <span style="font-family:Georgia,'Times New Roman',serif;font-size:13px;font-style:italic;color:{muted};padding-{align_side}:10px;">— {tagline}</span>
            </td>
          </tr>

          <tr>
            <td style="padding:22px 34px 0;text-align:{align};">
              <h1 style="margin:0;font-family:Georgia,'Times New Roman',serif;font-size:27px;line-height:1.24;font-weight:bold;color:{text};">{title}</h1>
            </td>
          </tr>

          <tr>
            <td style="padding:14px 34px 0;text-align:{align};font-family:-apple-system,'Segoe UI',Arial,sans-serif;">
              {greeting}
              <div style="font-size:16px;line-height:1.65;color:{text};">{body}</div>
              {stats}
              {cta}
            </td>
          </tr>

          <tr><td style="padding:26px 34px 0;"><div style="height:1px;background:{border};"></div></td></tr>

          <tr>
            <td style="padding:18px 34px 30px;text-align:{align};font-family:-apple-system,'Segoe UI',Arial,sans-serif;">
              <p style="margin:0;font-size:12px;line-height:1.5;color:{muted};">{footer_note}</p>
              {unsubscribe}
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#,
        surface = t.surface,
        card = t.card,
        border = t.border,
        text = t.text,
        muted = t.text_muted,
        accent = t.accent,
        tagline = t.tagline,
        // The tagline hangs off the wordmark, so its padding follows the
        // reading direction rather than always sitting on the left.
        align_side = if dir == "rtl" { "right" } else { "left" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample<'a>(locale: &'a str, theme: Option<&'a str>) -> Email<'a> {
        Email {
            locale,
            theme,
            title: "Ton paiement est parti",
            body: "42,50 € ont été envoyés sur ton Mobile Money.",
            recipient_name: Some("Ada"),
            stats: &[],
            cta_label: Some("Voir le détail"),
            cta_url: Some("https://skill-uv.com/wallet"),
            unsubscribe_url: None,
        }
    }

    #[test]
    fn it_renders_a_complete_document() {
        let html = render(sample("fr", None));
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Ton paiement est parti"));
        assert!(html.contains("https://skill-uv.com/wallet"));
        assert!(html.contains("lang=\"fr\""));
    }

    #[test]
    fn the_email_arrives_in_the_chosen_world() {
        let sakura = render(sample("fr", Some("sakura")));
        let arena = render(sample("fr", Some("arena")));

        assert!(sakura.contains("#fce7ea"), "sakura's blossom background");
        assert!(sakura.contains("sous les cerisiers"));
        assert!(arena.contains("#b91c1c"), "arena's heraldic red");
        assert!(arena.contains("dans l'arène"));
        assert_ne!(sakura, arena, "two worlds must not render identically");
    }

    #[test]
    fn every_world_renders() {
        for theme in email_theme::ALL {
            let html = render(sample("fr", Some(theme.name)));
            assert!(
                html.contains(theme.accent),
                "{}: accent missing",
                theme.name
            );
            assert!(
                html.contains(theme.surface),
                "{}: surface missing",
                theme.name
            );
            assert!(
                html.contains(theme.tagline),
                "{}: tagline missing",
                theme.name
            );
        }
    }

    #[test]
    fn an_unknown_world_falls_back_rather_than_breaking() {
        let html = render(sample("fr", Some("whatever-ships-next")));
        assert!(html.contains("l'atelier"), "falls back to the workshop");
        assert!(html.starts_with("<!doctype html>"));
    }

    #[test]
    fn arabic_flips_the_document_direction() {
        let html = render(sample("ar", Some("forge")));
        assert!(html.contains(r#"dir="rtl""#));
        assert!(
            html.contains("text-align:right"),
            "an RTL email that stays left-aligned is visibly broken"
        );
        assert!(
            html.contains("padding-right:10px"),
            "the tagline must hang off the correct side of the wordmark"
        );
    }

    #[test]
    fn latin_scripts_stay_left_to_right() {
        let html = render(sample("fr", None));
        assert!(html.contains(r#"dir="ltr""#));
        assert!(html.contains("text-align:left"));
    }

    #[test]
    fn user_supplied_text_cannot_inject_markup() {
        let html = render(Email {
            locale: "fr",
            theme: None,
            title: "<script>alert(1)</script>",
            body: "ok",
            recipient_name: Some("<img src=x onerror=alert(1)>"),
            stats: &[],
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
            theme: None,
            title: "t",
            body: "b",
            recipient_name: None,
            stats: &[],
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
            theme: None,
            title: "t",
            body: "b",
            recipient_name: None,
            stats: &[],
            cta_label: None,
            cta_url: None,
            unsubscribe_url: None,
        });
        assert!(!html.contains("border-radius:10px;background:"));
    }

    #[test]
    fn transactional_mail_offers_no_unsubscribe() {
        let html = render(sample("fr", None));
        assert!(!html.contains("unsubscribe"));
    }

    #[test]
    fn line_breaks_in_the_body_survive() {
        let html = render(Email {
            locale: "fr",
            theme: None,
            title: "t",
            body: "ligne un\nligne deux",
            recipient_name: None,
            stats: &[],
            cta_label: None,
            cta_url: None,
            unsubscribe_url: None,
        });
        assert!(html.contains("ligne un<br>ligne deux"));
    }
}
