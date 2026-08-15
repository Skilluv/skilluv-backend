//! The five worlds an email can arrive in.
//!
//! The frontend does not ship colour variants, it ships universes: the
//! smith's workshop, the lantern-lit night, the tournament, the copyist's
//! desk, the cherry blossom season. Someone picked one. Sending them a
//! grey-and-blue transactional email afterwards throws that away, and makes
//! the product feel like two different companies.
//!
//! So the email arrives in the theme they chose.
//!
//! ## Light only
//!
//! Each world has a dark and a light variant. Emails always use the light
//! one. A dark background is a deliberate choice inside an application
//! someone opened; in an inbox, between two white messages, it reads as a
//! rendering fault — and several clients force their own background anyway,
//! leaving pale text on white.
//!
//! ## Where the values come from
//!
//! `skilluv-frontend/src/app.css`, the `[data-theme='*-light']` blocks. They
//! are duplicated here rather than fetched: an email is rendered by a server
//! that cannot read the frontend's stylesheet, and a build-time dependency
//! between the two repositories would be a worse cure than the duplication.
//! When a palette changes there, it changes here.

/// One world's light palette, plus what it is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Matches `data-theme` in the frontend, without the `-light` suffix.
    pub name: &'static str,
    /// Page background.
    pub surface: &'static str,
    /// The card the message sits on.
    pub card: &'static str,
    pub border: &'static str,
    pub text: &'static str,
    pub text_muted: &'static str,
    /// The signature colour: the accent bar, the button.
    pub accent: &'static str,
    pub accent_fg: &'static str,
    /// Shown under the wordmark. Not decoration — it is what tells someone
    /// the message comes from the same place they chose this world in.
    pub tagline: &'static str,
}

/// The smith's workshop. Ochre and ember on parchment. The default.
pub const FORGE: Theme = Theme {
    name: "forge",
    surface: "#f4ede0",
    card: "#ffffff",
    border: "#c9b98e",
    text: "#1c1a17",
    text_muted: "#6b5b3f",
    accent: "#a04520",
    accent_fg: "#ffffff",
    tagline: "l'atelier",
};

/// Night by lantern light. Deep blue on warm parchment.
pub const VESPERAL: Theme = Theme {
    name: "vesperal",
    surface: "#f0e6d2",
    card: "#ffffff",
    border: "#b8a880",
    text: "#1a2340",
    text_muted: "#5c4d33",
    accent: "#c14e33",
    accent_fg: "#ffffff",
    tagline: "à la lanterne",
};

/// The tournament. Heraldic red.
pub const ARENA: Theme = Theme {
    name: "arena",
    surface: "#ebd9d4",
    card: "#ffffff",
    border: "#b89890",
    text: "#2d0f0f",
    text_muted: "#6b3a32",
    accent: "#b91c1c",
    accent_fg: "#ffffff",
    tagline: "dans l'arène",
};

/// The copyist's desk. Parchment and candle ink.
pub const SCRIPTORIUM: Theme = Theme {
    name: "scriptorium",
    surface: "#e8dcbf",
    card: "#ffffff",
    border: "#b39d70",
    text: "#24180f",
    text_muted: "#6b5638",
    accent: "#a04520",
    accent_fg: "#ffffff",
    tagline: "au scriptorium",
};

/// Cherry blossom season. Plum and blossom.
pub const SAKURA: Theme = Theme {
    name: "sakura",
    surface: "#fce7ea",
    card: "#ffffff",
    border: "#d8b4c8",
    text: "#1f1622",
    text_muted: "#6b3a5c",
    accent: "#d4739c",
    accent_fg: "#ffffff",
    tagline: "sous les cerisiers",
};

pub const ALL: &[Theme] = &[FORGE, VESPERAL, ARENA, SCRIPTORIUM, SAKURA];

/// Resolve a stored preference to a palette.
///
/// Accepts both `sakura` and `sakura-light`: the frontend stores whichever
/// variant is active, and which one someone happened to be using when the
/// preference was saved should not decide what their email looks like.
///
/// Anything unknown falls back to the workshop. A new theme shipped in the
/// frontend before it is added here sends a Forge email — plain, never
/// broken.
pub fn resolve(preference: Option<&str>) -> Theme {
    let Some(pref) = preference else {
        return FORGE;
    };
    let base = pref
        .trim()
        .trim_end_matches("-light")
        .trim_end_matches("-dark");
    ALL.iter()
        .find(|t| t.name.eq_ignore_ascii_case(base))
        .copied()
        .unwrap_or(FORGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_world_is_distinct() {
        // Five palettes that resolve to the same colours would be five names
        // for one theme.
        let accents: std::collections::HashSet<&str> = ALL.iter().map(|t| t.accent).collect();
        let surfaces: std::collections::HashSet<&str> = ALL.iter().map(|t| t.surface).collect();
        assert_eq!(surfaces.len(), ALL.len(), "two worlds share a background");
        assert!(accents.len() >= 4, "the accents barely differ");
    }

    #[test]
    fn the_light_suffix_is_accepted() {
        assert_eq!(resolve(Some("sakura")).name, "sakura");
        assert_eq!(resolve(Some("sakura-light")).name, "sakura");
        assert_eq!(resolve(Some("arena-light")).name, "arena");
    }

    #[test]
    fn an_unknown_theme_falls_back_to_the_workshop() {
        assert_eq!(resolve(Some("neon-cyberpunk")).name, "forge");
        assert_eq!(resolve(None).name, "forge");
        assert_eq!(resolve(Some("")).name, "forge");
    }

    #[test]
    fn every_palette_is_a_valid_colour() {
        for theme in ALL {
            for value in [
                theme.surface,
                theme.card,
                theme.border,
                theme.text,
                theme.text_muted,
                theme.accent,
                theme.accent_fg,
            ] {
                assert!(
                    value.starts_with('#') && (value.len() == 7 || value.len() == 4),
                    "{}: {value} is not a hex colour",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn text_is_dark_and_background_is_light() {
        // Emails use the light variant on purpose. A dark surface here would
        // read as a rendering fault in an inbox, and some clients force a
        // white background regardless, leaving pale text on white.
        fn luminance(hex: &str) -> u32 {
            let v = u32::from_str_radix(&hex[1..], 16).unwrap_or(0);
            let (r, g, b) = ((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff);
            (r * 299 + g * 587 + b * 114) / 1000
        }
        for theme in ALL {
            assert!(
                luminance(theme.surface) > 180,
                "{}: the surface is not light",
                theme.name
            );
            assert!(
                luminance(theme.text) < 90,
                "{}: the text is not dark enough to read on it",
                theme.name
            );
        }
    }
}
