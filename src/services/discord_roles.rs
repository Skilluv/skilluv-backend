//! Which Discord roles a person should hold, and how that becomes true.
//!
//! ## The split, and why the backend never touches Discord
//!
//! Only one process holds `DISCORD_BOT_TOKEN`: the bot. That token can create
//! channels, delete them and grant any role, and the HTTP backend has no
//! business holding a credential that powerful. So this module decides *what*
//! should be true and writes it to `discord_role_sync_queue`; the bot makes it
//! true. The notification queue already works that way.
//!
//! ## Why the declaration is `ops/discord/server.toml` and not a table
//!
//! That file already declares the server: 24 categories, 182 channels, the
//! roles. Putting the mapping anywhere else would create a second source of
//! truth to keep in step with the first — the exact failure this codebase has
//! already paid for once, when the admin panel held its own copy of a
//! capability list anchored to a CHECK that migration 0404 had deleted.
//!
//! A role rule changes once or twice a year. It deserves a pull request more
//! than it deserves a form.
//!
//! ## The compression
//!
//! The platform has 116 capabilities and 132 orientations. Discord caps a
//! server at 250 roles, and a human reads far fewer. So:
//!
//!   * **132 orientations collapse to 12 domain roles.** The orientation lives
//!     on the profile; what a Discord room is about is the domain. `@design`
//!     has to be mentionable — `@design-motion-designer` does not.
//!   * **~100 scoped capabilities collapse to one role per family**, through
//!     the `*` in `capability = "design_reviewer:*"`. What a member wants to
//!     know is who to ask, not which sub-family the grant names.
//!
//! ## What is deliberately not synced
//!
//! See [`NEVER_PUBLISHED`]. Discord roles are public — the member list shows
//! them to anybody who joins.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// The declaration, embedded at build time.
///
/// `include_str!` rather than reading the path at runtime: the bot and the
/// backend are two processes, and a file one of them cannot find would be a
/// startup failure at best and a silently empty mapping at worst. The
/// Dockerfile copies `ops/` for this reason.
const SERVER_TOML: &str = include_str!("../../ops/discord/server.toml");

/// Capabilities whose holders are never named on Discord.
///
/// Not an oversight and not a preference. A Discord role is visible to every
/// member of the server, so syncing these would publish — permanently, and to
/// anybody who joins — who decides plagiarism cases, who reads identity
/// documents and who triages security reports. Those are the three roles on
/// this platform whose holders somebody has a motive to pressure.
///
/// `admin` is here for a plainer reason: it is not a community standing, and a
/// server where the staff wear a badge invites every question to be addressed
/// to the badge instead of to the room.
pub const NEVER_PUBLISHED: &[&str] = &[
    "admin",
    "kyc_reviewer",
    "plagiarism_reviewer",
    "security_triager",
];

/// One role the server declares, and what makes somebody eligible for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleRule {
    pub name: String,
    /// `None` for roles this loop must never touch — the ones marked
    /// `manual = true`, which are yours to hand out.
    pub grant: Option<Grant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grant {
    /// Anybody who has declared a trade in this domain.
    Orientation { domain: String },
    /// Anybody holding a capability matching this pattern. A trailing `*`
    /// matches a whole scoped family: `design_reviewer:*`.
    Capability { pattern: String },
    /// Exactly this rank. Exclusive — see [`Grant::is_exclusive`].
    Rank { rank: String },
}

impl Grant {
    /// True when holding one value of this kind excludes the others.
    ///
    /// Only rank. Somebody is one rank at a time, so promoting to `artisan`
    /// has to take `ranger` back. Trades and capabilities accumulate, which is
    /// the whole shape of this platform and maps onto Discord without argument:
    /// a member's `roles` field is an array.
    pub fn is_exclusive(&self) -> bool {
        matches!(self, Grant::Rank { .. })
    }
}

/// Does a held capability satisfy a rule's pattern?
fn capability_matches(pattern: &str, held: &str) -> bool {
    match pattern.strip_suffix('*') {
        // `design_reviewer:*` matches `design_reviewer:motion` and must not
        // match `design_reviewer_elsewhere` — the prefix keeps its colon.
        Some(prefix) => held.starts_with(prefix),
        None => held == pattern,
    }
}

/// Every role the server declares, read from the declaration.
///
/// Roles marked `manual = true` come back with `grant: None`. They are listed
/// rather than dropped on purpose: the loop has to know they exist so it can
/// leave them alone. A loop that only knew what it grants could not tell "a
/// role I manage and should remove" from "a role somebody was given by hand",
/// and would strip the second.
pub fn rules() -> Result<Vec<RoleRule>, AppError> {
    let doc: toml::Value = toml::from_str(SERVER_TOML)
        .map_err(|e| AppError::Internal(format!("ops/discord/server.toml is not valid: {e}")))?;

    let mut out = Vec::new();
    let Some(roles) = doc.get("roles").and_then(|r| r.as_array()) else {
        return Ok(out);
    };

    for role in roles {
        let Some(name) = role.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        let grant = match role.get("grants_on").and_then(|g| g.as_str()) {
            Some("orientation") => {
                role.get("domain")
                    .and_then(|d| d.as_str())
                    .map(|domain| Grant::Orientation {
                        domain: domain.to_string(),
                    })
            }
            Some("capability") => role
                .get("capability")
                .and_then(|c| c.as_str())
                .map(|pattern| Grant::Capability {
                    pattern: pattern.to_string(),
                }),
            Some("rank") => role
                .get("rank")
                .and_then(|r| r.as_str())
                .map(|rank| Grant::Rank {
                    rank: rank.to_string(),
                }),
            _ => None,
        };
        out.push(RoleRule {
            name: name.to_string(),
            grant,
        });
    }
    Ok(out)
}

/// What this person has on the platform, in the three terms the rules read.
#[derive(Debug, Default, Clone)]
pub struct Standing {
    pub domains: Vec<String>,
    pub capabilities: Vec<String>,
    pub rank: Option<String>,
    /// False once the Discord link is gone. Every desired role is then empty,
    /// which is how a leaver is stripped rather than left decorated.
    pub linked: bool,
}

/// Read the three terms out of the database.
pub async fn standing(db: &PgPool, user_id: Uuid) -> Result<Standing, AppError> {
    let linked: Option<Option<String>> =
        sqlx::query_scalar("SELECT discord_user_id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    if !matches!(linked, Some(Some(ref s)) if !s.is_empty()) {
        return Ok(Standing::default());
    }

    // The domain, not the orientation: 132 of the second collapse into 12 of
    // the first, and the domain is what a Discord room is about.
    let domains: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT o.primary_domain
           FROM user_orientations uo
           JOIN orientations o ON o.id = uo.orientation_id
          WHERE uo.user_id = $1 AND uo.ended_at IS NULL",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let capabilities: Vec<String> = sqlx::query_scalar(
        "SELECT capability FROM user_capabilities
          WHERE user_id = $1
            AND revoked_at IS NULL
            AND (expires_at IS NULL OR expires_at > NOW())",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let rank: Option<String> = sqlx::query_scalar("SELECT rank FROM user_ranks WHERE user_id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?
        .flatten();

    Ok(Standing {
        domains,
        capabilities,
        rank,
        linked: true,
    })
}

/// The roles this standing earns, by name.
///
/// Pure, so it is testable without a database or a Discord server — which
/// matters, because the failure it can produce is somebody wearing an
/// authority they do not hold, and that is not a thing to discover in
/// production.
pub fn desired(rules: &[RoleRule], standing: &Standing) -> Vec<String> {
    if !standing.linked {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    for rule in rules {
        let Some(grant) = &rule.grant else { continue };
        let earned = match grant {
            Grant::Orientation { domain } => standing.domains.iter().any(|d| d == domain),
            Grant::Capability { pattern } => {
                // The deny-list is applied to what the rule asks for, not to
                // what the person holds: a rule naming a never-published
                // capability grants nothing, to anybody, ever.
                !NEVER_PUBLISHED
                    .iter()
                    .any(|denied| capability_matches(pattern, denied))
                    && standing
                        .capabilities
                        .iter()
                        .any(|held| capability_matches(pattern, held))
            }
            Grant::Rank { rank } => standing.rank.as_deref() == Some(rank.as_str()),
        };
        if earned && !out.contains(&rule.name) {
            out.push(rule.name.clone());
        }
    }
    out
}

/// Every role name the loop is allowed to remove.
///
/// Anything outside this set is somebody's decision — `@Incident Commander`,
/// `@AI Champion` — and the loop leaves it exactly where it found it.
pub fn managed(rules: &[RoleRule]) -> Vec<String> {
    rules
        .iter()
        .filter(|r| r.grant.is_some())
        .map(|r| r.name.clone())
        .collect()
}

/// Ask the bot to reconcile this person.
///
/// Idempotent while one is pending: the partial unique index of migration 0602
/// collapses repeats into an update of the reason. A single validated
/// deliverable can move a rank, grant a capability and fire three hooks; each
/// would otherwise queue the same work and the bot would issue the same
/// Discord writes four times against a rate-limited API.
pub async fn request_sync(db: &PgPool, user_id: Uuid, reason: &str) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO discord_role_sync_queue (user_id, reason)
         VALUES ($1, $2)
         ON CONFLICT (user_id) WHERE applied_at IS NULL
         DO UPDATE SET reason = EXCLUDED.reason, requested_at = NOW()",
    )
    .bind(user_id)
    .bind(reason)
    .execute(db)
    .await?;
    Ok(())
}

/// Ask for a sync, but never fail the caller because of it.
///
/// The callers are proof hooks running right after somebody's work was
/// validated. A Discord role is a nice-to-have; the validation is not. Losing
/// the second because the first could not be queued would be the wrong trade.
pub async fn request_sync_best_effort(db: &PgPool, user_id: Uuid, reason: &str) {
    if let Err(e) = request_sync(db, user_id, reason).await {
        tracing::warn!(user = %user_id, reason, error = %e, "could not queue Discord role sync");
    }
}

// ═══════════════════════════════════════════════════════════════════
// The diff
// ═══════════════════════════════════════════════════════════════════

/// What the loop will change for one member.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }
}

/// Compare what somebody should hold against what they do hold.
///
/// ## Why a diff and not a grant
///
/// The obvious loop grants what somebody has earned and stops. It is wrong
/// twice over:
///
///   * **Rank is exclusive.** Promotion to `artisan` has to take `ranger`
///     back. An additive loop leaves both, forever, and the member list
///     becomes a record of every rank anybody ever held.
///   * **Standing can be lost.** A revoked capability, an abandoned trade, an
///     unlinked account. Only a diff takes the role back.
///
/// `held` is every role the member wears, by name — including ones this
/// repository has never heard of. `managed` is the only set the loop may
/// remove from, which is what keeps a hand-granted `@Incident Commander` where
/// somebody put it.
///
/// Pure, and lives here rather than beside the Discord calls, because the two
/// failures it can produce — stripping a role nobody asked it to touch, and
/// leaving an authority somebody no longer holds — are not things to discover
/// on a live server.
pub fn diff(desired: &[String], held: &[String], managed: &[String]) -> Diff {
    use std::collections::HashSet;
    let desired_set: HashSet<&str> = desired.iter().map(String::as_str).collect();
    let held_set: HashSet<&str> = held.iter().map(String::as_str).collect();
    let managed_set: HashSet<&str> = managed.iter().map(String::as_str).collect();

    let mut add: Vec<String> = desired
        .iter()
        .filter(|r| !held_set.contains(r.as_str()))
        .cloned()
        .collect();

    // Only ever from the managed set. Everything else on that member belongs
    // to whoever put it there.
    let mut remove: Vec<String> = held
        .iter()
        .filter(|r| managed_set.contains(r.as_str()) && !desired_set.contains(r.as_str()))
        .cloned()
        .collect();

    add.sort();
    remove.sort();
    Diff { add, remove }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_promotion_takes_the_previous_rank_back() {
        let d = diff(
            &v(&["Artisan"]),
            &v(&["Ranger"]),
            &v(&["Apprenti", "Ranger", "Artisan", "Maitre", "Doyen"]),
        );
        assert_eq!(d.add, v(&["Artisan"]));
        assert_eq!(d.remove, v(&["Ranger"]));
    }

    #[test]
    fn a_hand_granted_role_is_never_touched() {
        // `@Incident Commander` means nothing on the platform and was given by
        // a person. It is held, it is not desired, and it must survive.
        let d = diff(
            &v(&["Doyen"]),
            &v(&["Incident Commander", "AI Champion"]),
            &v(&["Doyen", "Ranger"]),
        );
        assert_eq!(d.add, v(&["Doyen"]));
        assert!(
            d.remove.is_empty(),
            "stripped somebody else's decision: {:?}",
            d.remove
        );
    }

    #[test]
    fn losing_standing_takes_the_role_back() {
        let d = diff(
            &v(&[]),
            &v(&["Mentor", "Designer"]),
            &v(&["Mentor", "Designer"]),
        );
        assert!(d.add.is_empty());
        assert_eq!(d.remove, v(&["Designer", "Mentor"]));
    }

    #[test]
    fn a_steady_state_sends_nothing() {
        // Every tick recomputes. Without this, standing still would cost one
        // Discord write per role per member per tick, against an API that
        // rate-limits per guild.
        let d = diff(
            &v(&["Doyen", "Designer"]),
            &v(&["Designer", "Doyen", "Incident Commander"]),
            &v(&["Doyen", "Designer", "Mentor"]),
        );
        assert!(d.is_empty(), "{d:?}");
    }

    #[test]
    fn a_role_the_repository_never_heard_of_is_left_alone() {
        // Somebody creates `@Traducteur` by hand. Not in the declaration, so
        // not managed, so not the loop's to remove.
        let d = diff(&v(&["Designer"]), &v(&["Traducteur"]), &v(&["Designer"]));
        assert_eq!(d.add, v(&["Designer"]));
        assert!(d.remove.is_empty());
    }

    fn standing_of(domains: &[&str], caps: &[&str], rank: Option<&str>) -> Standing {
        Standing {
            domains: domains.iter().map(|s| s.to_string()).collect(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            rank: rank.map(|s| s.to_string()),
            linked: true,
        }
    }

    #[test]
    fn the_declaration_parses_and_declares_the_four_kinds() {
        let rules = rules().expect("server.toml parses");
        assert!(rules.len() > 30, "{}", rules.len());

        let count = |f: fn(&Grant) -> bool| {
            rules
                .iter()
                .filter_map(|r| r.grant.as_ref())
                .filter(|g| f(g))
                .count()
        };
        assert_eq!(
            count(|g| matches!(g, Grant::Orientation { .. })),
            12,
            "one per domain"
        );
        assert_eq!(
            count(|g| matches!(g, Grant::Rank { .. })),
            5,
            "apprenti through doyen"
        );
        assert!(count(|g| matches!(g, Grant::Capability { .. })) >= 12);

        // The manual ones are listed, with no grant. Dropping them would leave
        // the loop unable to tell "mine to remove" from "somebody's decision".
        assert!(
            rules
                .iter()
                .any(|r| r.grant.is_none() && r.name.contains("Champion"))
        );
    }

    #[test]
    fn capabilities_accumulate_because_discord_roles_do() {
        // The premise the whole design rests on: a Discord member's `roles`
        // field is an array. Somebody who is a designer, a coder, a mentor and
        // a reviewer in two domains wears all of it at once.
        let rules = rules().unwrap();
        let got = desired(
            &rules,
            &standing_of(
                &["design", "code"],
                &["mentor", "design_reviewer:motion", "code_reviewer:web"],
                Some("artisan"),
            ),
        );
        assert!(got.len() >= 5, "{got:?}");
        assert!(got.iter().any(|r| r == "Artisan"), "{got:?}");
        assert!(got.iter().any(|r| r == "Designer"), "{got:?}");
        assert!(got.iter().any(|r| r == "Developer"), "{got:?}");
    }

    #[test]
    fn a_scoped_family_is_one_role_not_a_hundred() {
        // `design_reviewer:*` has fourteen rows in `capability_catalog`. A
        // member wants to know who to ask, not which sub-family a grant names.
        let rules = rules().unwrap();
        let one = desired(&rules, &standing_of(&[], &["design_reviewer:motion"], None));
        let two = desired(
            &rules,
            &standing_of(
                &[],
                &["design_reviewer:motion", "design_reviewer:brand"],
                None,
            ),
        );
        assert_eq!(one, two, "two scopes of one family must be one role");
        assert_eq!(one.len(), 1, "{one:?}");
    }

    #[test]
    fn the_pattern_does_not_match_past_its_family() {
        assert!(capability_matches(
            "design_reviewer:*",
            "design_reviewer:brand"
        ));
        assert!(!capability_matches(
            "design_reviewer:*",
            "design_reviewer_elsewhere"
        ));
        assert!(capability_matches("mentor", "mentor"));
        assert!(!capability_matches("mentor", "mentor_something"));
    }

    #[test]
    fn a_rank_is_exclusive_and_the_others_are_not() {
        let rules = rules().unwrap();
        assert!(
            rules
                .iter()
                .filter_map(|r| r.grant.as_ref())
                .filter(|g| matches!(g, Grant::Rank { .. }))
                .all(Grant::is_exclusive)
        );

        // One at a time. Promotion must take the old one back, which is why
        // the worker computes a diff rather than only granting.
        assert_eq!(
            desired(&rules, &standing_of(&[], &[], Some("artisan"))),
            vec!["Artisan".to_string()]
        );
        assert_eq!(
            desired(&rules, &standing_of(&[], &[], Some("doyen"))),
            vec!["Doyen".to_string()]
        );
    }

    #[test]
    fn the_roles_that_name_a_target_are_never_published() {
        // A Discord role is visible to every member. Publishing who decides
        // plagiarism cases, who reads identity documents and who triages
        // security reports names the people somebody has a motive to pressure.
        let rules = rules().unwrap();
        for sensitive in NEVER_PUBLISHED {
            let got = desired(&rules, &standing_of(&[], &[sensitive], None));
            assert!(
                got.is_empty(),
                "holding {sensitive} produced {got:?} — a public list of targets"
            );
        }
    }

    #[test]
    fn an_unlinked_account_wears_nothing() {
        // Unlinking has to strip, not freeze. Somebody who leaves keeping
        // `@Doyen` is an authority the platform no longer backs, pointing at a
        // person no longer connected to any account.
        let rules = rules().unwrap();
        let mut s = standing_of(&["design"], &["mentor"], Some("doyen"));
        assert!(!desired(&rules, &s).is_empty());
        s.linked = false;
        assert!(desired(&rules, &s).is_empty());
    }

    #[test]
    fn the_loop_never_claims_a_role_it_does_not_grant() {
        // The six `manual = true` roles mean nothing on the platform and are
        // handed out by a person. The loop must not be able to remove them.
        let rules = rules().unwrap();
        let managed = managed(&rules);
        for manual in ["AI Champion", "Incident Commander", "Ops Champion"] {
            assert!(
                !managed.contains(&manual.to_string()),
                "{manual} is somebody's decision, not the loop's"
            );
        }
        assert!(managed.contains(&"Doyen".to_string()));
    }
}
