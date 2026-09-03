//! What somebody gets around a brief so they are not stranded on it.
//!
//! A challenge says what to do. It does not say where the thing is documented,
//! where to ask when you are stuck, or what the last person who got stuck was
//! stuck on. For somebody arriving from a bootcamp, a degree or a career
//! change, that surrounding material is the difference between starting and
//! closing the tab.
//!
//! It is not the answer. Every resource here is a link somebody else hosts,
//! and finding the way through it is still the learner's work — what is
//! removed is the half-hour of guessing which words to type, which selects for
//! people who already knew how to search rather than for people who can do the
//! work.
//!
//! ## Why it shrinks
//!
//! An apprenti gets all of it. A doyen gets none of it, because handing a
//! fifteen-year practitioner a link to the official docs is noise, and noise on
//! every page teaches people to stop reading the page. The ladder
//! `apprenti → ranger → artisan → maitre → doyen` already exists (P17.4) and
//! this reads it rather than inventing a second notion of experience.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::services::ranks;

/// Where to start reading, for one challenge.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Resource {
    /// `documentation`, `course`, `article`, `video`, `community`,
    /// `repository`.
    pub kind: String,
    pub title: String,
    pub url: String,
    /// The language the resource itself is in — not the caller's. A French
    /// reader has to be able to see which of these they can actually read.
    pub language: String,
    pub summary: String,
    /// What it takes to actually reach it: free tier, account needed, course
    /// auditable without paying.
    pub access_note: String,
    /// Whatever a licence requires to travel with the use. Empty for a plain
    /// link, which is most of them.
    pub attribution: String,
}

/// Where to ask when the reading has not been enough.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct HelpChannel {
    /// `forum` or `discord`.
    pub kind: String,
    pub label: String,
    /// Where the front sends them. A path for our own surfaces.
    pub target: String,
}

/// Everything around the brief, sized to who is asking.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Guidance {
    /// The rank this was sized for. `apprenti` for somebody not signed in,
    /// because a visitor reading a challenge is being shown what a beginner
    /// would get.
    pub sized_for: String,
    /// True when the caller is far enough along that the platform stops
    /// explaining. A doyen gets an empty guidance block and that is the
    /// intent, not a gap.
    pub self_sufficient: bool,
    /// Ordered: what to read first is first.
    pub resources: Vec<Resource>,
    /// Where to ask. Empty for a self-sufficient caller.
    pub help: Vec<HelpChannel>,
    /// How many forum threads already exist about this challenge. The number
    /// matters on its own: "four people asked before you" is the single most
    /// reassuring thing a stuck beginner can read.
    pub discussions: i64,
}

/// How many resources a rank is shown.
///
/// A ladder rather than a flag, because the drop from "everything" to
/// "nothing" in one step is what makes guidance feel like a wall being pulled
/// away. `None` means every resource on the challenge.
fn budget_for(rank: &str) -> Option<usize> {
    match rank {
        ranks::RANK_APPRENTI => None,
        ranks::RANK_RANGER => None,
        ranks::RANK_ARTISAN => Some(3),
        ranks::RANK_MAITRE => Some(1),
        ranks::RANK_DOYEN => Some(0),
        // A rank nothing recognises is treated as the start of the ladder:
        // showing too much to somebody experienced wastes their time, showing
        // too little to a beginner loses them.
        _ => None,
    }
}

/// The guidance for one challenge and one caller.
///
/// `user_id` is `None` for an anonymous reader, who is shown what an apprenti
/// would get — a challenge page has to make sense before anybody signs up.
pub async fn for_challenge(
    db: &PgPool,
    challenge_id: Uuid,
    user_id: Option<Uuid>,
    locale: &str,
) -> Result<Guidance, AppError> {
    let rank = match user_id {
        Some(id) => ranks::effective_rank(db, id).await?,
        None => ranks::RANK_APPRENTI.to_string(),
    };
    let budget = budget_for(&rank);
    let self_sufficient = budget == Some(0);

    let discussions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM posts
          WHERE challenge_id = $1 AND deleted_at IS NULL",
    )
    .bind(challenge_id)
    .fetch_one(db)
    .await?;

    if self_sufficient {
        return Ok(Guidance {
            sized_for: rank,
            self_sufficient: true,
            resources: Vec::new(),
            help: Vec::new(),
            discussions,
        });
    }

    // Ordered by the curator, then by whether the caller can read it. A
    // French reader sees the French resources first and the English ones
    // after — never instead: most of what is worth linking is in English, and
    // hiding it would be a worse service than showing it second.
    let mut resources: Vec<Resource> = sqlx::query_as(
        "SELECT kind, title, url, language, summary, access_note, attribution
           FROM challenge_resources
          WHERE challenge_id = $1
          ORDER BY (language = $2) DESC, sort_order, title",
    )
    .bind(challenge_id)
    .bind(locale)
    .fetch_all(db)
    .await?;

    if let Some(n) = budget {
        resources.truncate(n);
    }

    let help = vec![
        HelpChannel {
            kind: "forum".to_string(),
            label: "Ask on the forum".to_string(),
            target: format!("/api/forum/posts?challenge_id={challenge_id}"),
        },
        HelpChannel {
            kind: "discord".to_string(),
            label: "Ask in the domain's Discord room".to_string(),
            target: "/api/discord/invite".to_string(),
        },
    ];

    Ok(Guidance {
        sized_for: rank,
        self_sufficient: false,
        resources,
        help,
        discussions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ladder narrows in one direction and ends at nothing.
    ///
    /// Written as an ordering rather than as five equalities: what matters is
    /// that nobody further along is shown more than somebody earlier, which is
    /// the property a future sixth rank has to preserve.
    #[test]
    fn guidance_only_ever_narrows() {
        let ladder = ranks::rank_order();
        let mut previous = usize::MAX;
        for rank in ladder {
            let budget = budget_for(rank).unwrap_or(usize::MAX);
            assert!(
                budget <= previous,
                "{rank} is shown more than the rank before it"
            );
            previous = budget;
        }
        assert_eq!(
            budget_for(ranks::RANK_DOYEN),
            Some(0),
            "the top of the ladder is where the platform stops explaining"
        );
    }

    /// An unknown rank is treated as a beginner. Showing too much to somebody
    /// experienced costs them a scroll; showing too little to a beginner loses
    /// them.
    #[test]
    fn an_unknown_rank_is_given_everything() {
        assert_eq!(budget_for("something-new"), None);
    }
}
