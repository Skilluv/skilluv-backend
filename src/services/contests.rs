//! Contests a company pays for.
//!
//! Six shapes in the backlog, one table, because they differ in exactly one
//! thing: what happens to the winner. Three of them end in paid work, and
//! paid work is an engagement — so the outcome is a foreign key rather than
//! three more tables.
//!
//! ## What the entrants were told
//!
//! A recruiting contest pays in interviews. That is not a smaller version of
//! a prize, it is a different offer, and the code refuses to attach a cash
//! prize to one — otherwise the thing people entered would not be the thing
//! that ran.
//!
//! ## The rank is for everybody
//!
//! Judging sets a rank on every submission it looks at, not only the ones
//! that make the shortlist. Somebody who came fourth out of forty is owed
//! that number: without it they are indistinguishable from the people who
//! never entered, which is the opposite of what a proof platform is for.

use bigdecimal::BigDecimal;
use bigdecimal::num_traits::Signed;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

pub const KINDS: &[&str] = &[
    "recruiting",
    "award",
    "product_led",
    "corporate_internal",
    "migration",
];

pub const MODES: &[&str] = &["self_serve", "managed"];

/// What a recruiting contest costs the client.
///
/// Self-serve is a setup fee plus a charge for each shortlisted person the
/// company actually talks to. Managed is a flat campaign fee — Skilluv does
/// the sourcing, and charging per contact on top would bill twice for the
/// same work.
///
/// The success fee is not here: it depends on a salary nobody knows yet, and
/// folding an estimate into the quote would make the quote a guess.
pub fn contest_cost(
    mode: &str,
    setup_fee: &BigDecimal,
    per_contact_fee: &BigDecimal,
    contacts: i64,
    managed_fee: &BigDecimal,
) -> BigDecimal {
    match mode {
        "managed" => managed_fee.clone(),
        _ => setup_fee + per_contact_fee * BigDecimal::from(contacts.max(0)),
    }
}

/// How an award's pool divides, first place first.
///
/// The first prize is stated; what remains is split evenly between the rest
/// of the shortlist. Evenly rather than by a curve because a curve is a
/// negotiation and this is a default — a contest wanting something else
/// states every prize.
///
/// The last place absorbs the rounding, for the same reason it does when a
/// milestone pays a team: a centime kept back by the platform is a centime
/// taken from somebody who earned it.
pub fn prize_split(
    prize_first: &BigDecimal,
    pool_total: &BigDecimal,
    winners: usize,
) -> Vec<BigDecimal> {
    if winners == 0 {
        return vec![];
    }
    if winners == 1 || pool_total <= prize_first {
        return vec![prize_first.clone()];
    }

    let rest = pool_total - prize_first;
    let others = winners - 1;
    let each = (&rest / BigDecimal::from(others as i64))
        .with_scale_round(2, bigdecimal::RoundingMode::Down);

    let mut prizes = vec![prize_first.clone()];
    let mut given = BigDecimal::from(0);
    for _ in 0..others - 1 {
        given += &each;
        prizes.push(each.clone());
    }
    prizes.push(&rest - &given);
    prizes
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Contest {
    pub id: Uuid,
    pub slug: String,
    pub enterprise_id: Uuid,
    pub kind: String,
    pub title: String,
    pub brief_md: String,
    pub orientation_target: Option<String>,
    pub domain_target: Option<String>,
    pub visibility: String,
    pub opens_at: Option<chrono::DateTime<chrono::Utc>>,
    pub submissions_deadline: chrono::DateTime<chrono::Utc>,
    pub shortlist_size: i16,
    pub mode: Option<String>,
    pub setup_fee: Option<BigDecimal>,
    pub per_candidate_contact_fee: Option<BigDecimal>,
    pub managed_campaign_fee: Option<BigDecimal>,
    pub success_fee_percent: Option<BigDecimal>,
    pub prize_first: Option<BigDecimal>,
    pub prize_pool_total: Option<BigDecimal>,
    pub jury_composition: serde_json::Value,
    pub internal_employees_count: Option<i16>,
    pub external_talents_count: Option<i16>,
    pub per_external_talent_fee: Option<BigDecimal>,
    pub orchestration_fee: BigDecimal,
    pub currency: String,
    pub outcome_engagement_id: Option<Uuid>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

const CONTEST_SELECT: &str = r#"
    SELECT id, slug, enterprise_id, kind, title, brief_md, orientation_target,
           domain_target, visibility, opens_at, submissions_deadline,
           shortlist_size, mode, setup_fee, per_candidate_contact_fee,
           managed_campaign_fee, success_fee_percent, prize_first,
           prize_pool_total, jury_composition, internal_employees_count,
           external_talents_count, per_external_talent_fee, orchestration_fee,
           currency, outcome_engagement_id, status, created_at
      FROM enterprise_contests
"#;

#[derive(Debug, Clone, Deserialize)]
pub struct ContestInput {
    pub slug: String,
    pub kind: String,
    pub title: String,
    pub brief_md: String,
    #[serde(default)]
    pub orientation_target: Option<String>,
    #[serde(default)]
    pub domain_target: Option<String>,
    #[serde(default)]
    pub difficulty_tier: Option<i16>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub opens_at: Option<chrono::DateTime<chrono::Utc>>,
    pub submissions_deadline: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub shortlist_size: Option<i16>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub setup_fee: Option<BigDecimal>,
    #[serde(default)]
    pub per_candidate_contact_fee: Option<BigDecimal>,
    #[serde(default)]
    pub managed_campaign_fee: Option<BigDecimal>,
    #[serde(default)]
    pub success_fee_percent: Option<BigDecimal>,
    #[serde(default)]
    pub prize_first: Option<BigDecimal>,
    #[serde(default)]
    pub prize_pool_total: Option<BigDecimal>,
    #[serde(default)]
    pub marketing_budget: Option<BigDecimal>,
    #[serde(default)]
    pub jury_composition: Option<serde_json::Value>,
    #[serde(default)]
    pub internal_employees_count: Option<i16>,
    #[serde(default)]
    pub external_talents_count: Option<i16>,
    #[serde(default)]
    pub external_talents_specialization: Vec<String>,
    #[serde(default)]
    pub per_external_talent_fee: Option<BigDecimal>,
    #[serde(default)]
    pub current_stack_md: Option<String>,
    #[serde(default)]
    pub target_stack_md: Option<String>,
    #[serde(default)]
    pub orchestration_fee: Option<BigDecimal>,
    #[serde(default = "eur")]
    pub currency: String,
}

fn eur() -> String {
    "EUR".into()
}

/// The database speaks in constraint names; this says the same in words the
/// person filling in the brief can act on.
fn shape_error(e: sqlx::Error) -> AppError {
    let message = e.to_string();
    for (marker, said) in [
        (
            "a_recruiting_contest_says_how_it_is_run",
            "say whether this is self-serve or managed — the two are billed \
             differently, and a contest that says neither cannot be invoiced at all",
        ),
        (
            "a_recruiting_contest_pays_in_interviews",
            "the prize for a recruiting contest is an interview. A cash prize bolted \
             on makes it a different product wearing the same name, and the entrants \
             would have been told the wrong thing",
        ),
        (
            "an_award_has_a_prize",
            "an award challenge needs a first prize and a pool at least as large. \
             Without one it is a call for free work at scale",
        ),
        (
            "a_migration_contest_names_both_stacks",
            "say what you are migrating from and to — an approach cannot be proposed \
             against a blank",
        ),
        (
            "a_corporate_hackathon_mixes_people",
            "say how many of your own people and how many from Skilluv, and what an \
             outside place costs. The mix is the point; without outsiders you are \
             running your own event",
        ),
        ("enterprise_contests_slug_key", "that slug is already taken"),
        (
            "a_window_runs_forward",
            "the deadline has to come after the opening",
        ),
    ] {
        if message.contains(marker) {
            return AppError::Validation(said.into());
        }
    }
    AppError::from(e)
}

pub async fn open(
    db: &PgPool,
    enterprise_id: Uuid,
    author: Uuid,
    input: ContestInput,
) -> Result<Contest, AppError> {
    if !KINDS.contains(&input.kind.as_str()) {
        return Err(AppError::Validation(format!(
            "kind must be one of: {}",
            KINDS.join(", ")
        )));
    }
    if let Some(mode) = &input.mode
        && !MODES.contains(&mode.as_str())
    {
        return Err(AppError::Validation(format!(
            "mode must be one of: {}",
            MODES.join(", ")
        )));
    }
    if input.title.trim().is_empty() || input.brief_md.trim().is_empty() {
        return Err(AppError::Validation(
            "a contest needs a title and a brief — people are being asked to spend \
             days on this"
                .into(),
        ));
    }
    crate::validators::check_max_len(&input.title, "title", 200)?;
    crate::validators::check_max_len(&input.brief_md, "brief_md", 20_000)?;

    if let Some(slug) = Some(input.slug.trim())
        && (!(3..=80).contains(&slug.len())
            || !slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
    {
        return Err(AppError::Validation(
            "the slug must be 3 to 80 characters of lowercase letters, digits and dashes".into(),
        ));
    }

    if let Some(target) = &input.orientation_target {
        let resolved: Option<Uuid> = sqlx::query_scalar("SELECT resolve_orientation($1)")
            .bind(target)
            .fetch_one(db)
            .await?;
        if resolved.is_none() {
            return Err(AppError::Validation(format!(
                "'{target}' is not a trade Skilluv knows — a contest aimed at nobody \
                 reaches nobody"
            )));
        }
    }

    if input.submissions_deadline <= chrono::Utc::now() {
        return Err(AppError::Validation("the deadline is in the past".into()));
    }

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO enterprise_contests
            (slug, enterprise_id, kind, title, brief_md, orientation_target,
             domain_target, difficulty_tier, visibility, opens_at,
             submissions_deadline, shortlist_size, mode, setup_fee,
             per_candidate_contact_fee, managed_campaign_fee, success_fee_percent,
             prize_first, prize_pool_total, marketing_budget, jury_composition,
             internal_employees_count, external_talents_count,
             external_talents_specialization, per_external_talent_fee,
             current_stack_md, target_stack_md, orchestration_fee, currency,
             created_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,COALESCE($9,'public'),$10,$11,
                COALESCE($12,3),$13,$14,$15,$16,$17,$18,$19,$20,
                COALESCE($21,'[]'::jsonb),$22,$23,$24,$25,$26,$27,
                COALESCE($28,0),$29,$30)
        RETURNING id
        "#,
    )
    .bind(input.slug.trim())
    .bind(enterprise_id)
    .bind(&input.kind)
    .bind(input.title.trim())
    .bind(input.brief_md.trim())
    .bind(input.orientation_target.as_deref())
    .bind(input.domain_target.as_deref())
    .bind(input.difficulty_tier)
    .bind(input.visibility.as_deref())
    .bind(input.opens_at)
    .bind(input.submissions_deadline)
    .bind(input.shortlist_size)
    .bind(input.mode.as_deref())
    .bind(input.setup_fee.as_ref())
    .bind(input.per_candidate_contact_fee.as_ref())
    .bind(input.managed_campaign_fee.as_ref())
    .bind(input.success_fee_percent.as_ref())
    .bind(input.prize_first.as_ref())
    .bind(input.prize_pool_total.as_ref())
    .bind(input.marketing_budget.as_ref())
    .bind(input.jury_composition.as_ref())
    .bind(input.internal_employees_count)
    .bind(input.external_talents_count)
    .bind(&input.external_talents_specialization)
    .bind(input.per_external_talent_fee.as_ref())
    .bind(input.current_stack_md.as_deref())
    .bind(input.target_stack_md.as_deref())
    .bind(input.orchestration_fee.as_ref())
    .bind(&input.currency)
    .bind(author)
    .fetch_one(db)
    .await
    .map_err(shape_error)?;

    by_id(db, id).await
}

pub async fn by_id(db: &PgPool, id: Uuid) -> Result<Contest, AppError> {
    let sql = format!("{CONTEST_SELECT} WHERE id = $1");
    sqlx::query_as::<_, Contest>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("contest not found".into()))
}

pub async fn by_slug(db: &PgPool, slug: &str) -> Result<Contest, AppError> {
    let sql = format!("{CONTEST_SELECT} WHERE slug = $1");
    sqlx::query_as::<_, Contest>(sqlx::AssertSqlSafe(sql))
        .bind(slug)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NotFound("contest not found".into()))
}

/// What anybody can enter.
///
/// Invitation-only contests are absent by construction rather than filtered
/// in the caller: a private hiring search leaking into a public list is the
/// failure this table exists to avoid.
pub async fn open_contests(db: &PgPool, kind: Option<&str>) -> Result<Vec<Contest>, AppError> {
    let sql = format!(
        "{CONTEST_SELECT} WHERE status = 'submissions_open'
            AND visibility = 'public'
            AND submissions_deadline > NOW()
            AND ($1::TEXT IS NULL OR kind = $1)
          ORDER BY submissions_deadline LIMIT 100"
    );
    let rows = sqlx::query_as::<_, Contest>(sqlx::AssertSqlSafe(sql))
        .bind(kind)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn for_enterprise(db: &PgPool, enterprise_id: Uuid) -> Result<Vec<Contest>, AppError> {
    let sql = format!("{CONTEST_SELECT} WHERE enterprise_id = $1 ORDER BY created_at DESC");
    let rows = sqlx::query_as::<_, Contest>(sqlx::AssertSqlSafe(sql))
        .bind(enterprise_id)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Move a contest to a new status.
pub async fn set_status(db: &PgPool, id: Uuid, status: &str) -> Result<Contest, AppError> {
    const STATUSES: &[&str] = &[
        "draft",
        "published",
        "submissions_open",
        "judging",
        "shortlist_ready",
        "interviews_ongoing",
        "concluded",
        "cancelled",
    ];
    if !STATUSES.contains(&status) {
        return Err(AppError::Validation(format!(
            "status must be one of: {}",
            STATUSES.join(", ")
        )));
    }

    sqlx::query("UPDATE enterprise_contests SET status = $2 WHERE id = $1")
        .bind(id)
        .bind(status)
        .execute(db)
        .await
        .map_err(shape_error)?;
    by_id(db, id).await
}

// ═══════════════════════════════════════════════════════════════════
// Invitations
// ═══════════════════════════════════════════════════════════════════

pub async fn invite(db: &PgPool, contest_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO contest_invitations (contest_id, talent_user_id) VALUES ($1, $2)
         ON CONFLICT (contest_id, talent_user_id) DO NOTHING",
    )
    .bind(contest_id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn respond_to_invitation(
    db: &PgPool,
    contest_id: Uuid,
    user_id: Uuid,
    accept: bool,
) -> Result<(), AppError> {
    let done = sqlx::query(
        "UPDATE contest_invitations
            SET accepted_at = CASE WHEN $3 THEN NOW() END,
                declined_at = CASE WHEN $3 THEN NULL ELSE NOW() END
          WHERE contest_id = $1 AND talent_user_id = $2",
    )
    .bind(contest_id)
    .bind(user_id)
    .bind(accept)
    .execute(db)
    .await?;

    if done.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "you have no invitation to this contest".into(),
        ));
    }
    Ok(())
}

async fn is_invited(db: &PgPool, contest_id: Uuid, user_id: Uuid) -> Result<bool, AppError> {
    let found: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM contest_invitations
              WHERE contest_id = $1 AND talent_user_id = $2 AND declined_at IS NULL
         )",
    )
    .bind(contest_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(found)
}

// ═══════════════════════════════════════════════════════════════════
// Submissions
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Submission {
    pub id: Uuid,
    pub talent_user_id: Uuid,
    pub username: String,
    pub deliverable_url: String,
    pub notes_md: Option<String>,
    pub final_rank: Option<i32>,
    pub shortlisted: bool,
    pub judge_notes: Option<String>,
    pub interview_completed: bool,
    pub hired: bool,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

pub async fn submissions(db: &PgPool, contest_id: Uuid) -> Result<Vec<Submission>, AppError> {
    let rows = sqlx::query_as::<_, Submission>(
        "SELECT s.id, s.talent_user_id, u.username, s.deliverable_url, s.notes_md,
                s.final_rank, s.shortlisted, s.judge_notes, s.interview_completed,
                s.hired, s.submitted_at
           FROM contest_submissions s
           JOIN users u ON u.id = s.talent_user_id
          WHERE s.contest_id = $1
          ORDER BY s.final_rank NULLS LAST, s.submitted_at",
    )
    .bind(contest_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Enter a contest.
pub async fn submit(
    db: &PgPool,
    contest_id: Uuid,
    user_id: Uuid,
    deliverable_url: &str,
    notes_md: Option<&str>,
) -> Result<Uuid, AppError> {
    let contest = by_id(db, contest_id).await?;

    if contest.status != "submissions_open" {
        return Err(AppError::Validation(format!(
            "this contest is {} and is not taking entries",
            contest.status
        )));
    }
    if contest.submissions_deadline < chrono::Utc::now() {
        return Err(AppError::Validation("the deadline has passed".into()));
    }
    if !deliverable_url.starts_with("https://") {
        return Err(AppError::Validation(
            "the entry has to be reachable over https — a judge cannot open a link \
             that is not there"
                .into(),
        ));
    }

    if contest.visibility == "invitation_only" && !is_invited(db, contest_id, user_id).await? {
        // Not found rather than forbidden: a private hiring search should not
        // confirm its own existence to somebody who guessed the slug.
        return Err(AppError::NotFound("contest not found".into()));
    }

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO contest_submissions
            (contest_id, talent_user_id, deliverable_url, notes_md)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (contest_id, talent_user_id) DO UPDATE
             SET deliverable_url = EXCLUDED.deliverable_url,
                 notes_md = EXCLUDED.notes_md,
                 submitted_at = NOW()
         RETURNING id",
    )
    .bind(contest_id)
    .bind(user_id)
    .bind(deliverable_url.trim())
    .bind(notes_md.map(str::trim).filter(|n| !n.is_empty()))
    .fetch_one(db)
    .await?;

    Ok(id)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verdict {
    pub submission_id: Uuid,
    pub final_rank: i32,
    #[serde(default)]
    pub judge_notes: Option<String>,
}

/// Rank the field, in one pass.
///
/// The whole judged set at once rather than one verdict at a time: ranks are
/// unique per contest, so a submission moved from third to second has to
/// displace whoever was there. Doing it row by row would fail halfway and
/// leave a shortlist that is neither the old one nor the new.
///
/// The shortlist is the first `shortlist_size` ranks. It is derived rather
/// than ticked by hand so that the number the client agreed to is the number
/// they get.
pub async fn judge(
    db: &PgPool,
    contest_id: Uuid,
    verdicts: Vec<Verdict>,
) -> Result<Vec<Submission>, AppError> {
    let contest = by_id(db, contest_id).await?;
    if verdicts.is_empty() {
        return Err(AppError::Validation("no verdicts given".into()));
    }

    let mut ranks: Vec<i32> = verdicts.iter().map(|v| v.final_rank).collect();
    ranks.sort_unstable();
    ranks.dedup();
    if ranks.len() != verdicts.len() {
        return Err(AppError::Validation(
            "two entries share a rank. A shortlist drawn from a tie is arbitrary, and \
             the person left out has no way to see why."
                .into(),
        ));
    }

    let mut tx = db.begin().await?;

    // Clear first: a rank is unique per contest, so re-ranking has to release
    // the old numbers before it claims the new ones.
    sqlx::query(
        "UPDATE contest_submissions SET final_rank = NULL, shortlisted = FALSE
          WHERE contest_id = $1",
    )
    .bind(contest_id)
    .execute(&mut *tx)
    .await?;

    for verdict in &verdicts {
        let shortlisted = verdict.final_rank <= contest.shortlist_size as i32;
        let done = sqlx::query(
            "UPDATE contest_submissions
                SET final_rank = $3, shortlisted = $4, judge_notes = $5,
                    judged_at = NOW()
              WHERE contest_id = $1 AND id = $2",
        )
        .bind(contest_id)
        .bind(verdict.submission_id)
        .bind(verdict.final_rank)
        .bind(shortlisted)
        .bind(verdict.judge_notes.as_deref())
        .execute(&mut *tx)
        .await?;

        if done.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "submission {} is not in this contest",
                verdict.submission_id
            )));
        }
    }

    sqlx::query("UPDATE enterprise_contests SET status = 'shortlist_ready' WHERE id = $1")
        .bind(contest_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    // The shortlist earns an attestation whether or not anybody is hired.
    // That is the point of it: a company with a real vacancy put this person
    // in its last few, which is harder to claim than a certificate.
    attest_shortlist(db, &contest).await?;

    submissions(db, contest_id).await
}

/// Issue a finalist attestation to everybody on the shortlist.
///
/// One insert per person rather than one statement: each attestation carries
/// its own verification code, and a code shared between two people would let
/// either of them verify as the other.
async fn attest_shortlist(db: &PgPool, contest: &Contest) -> Result<(), AppError> {
    let company: String = sqlx::query_scalar("SELECT company_name FROM enterprises WHERE id = $1")
        .bind(contest.enterprise_id)
        .fetch_one(db)
        .await?;

    let finalists: Vec<Uuid> = sqlx::query_scalar(
        "SELECT talent_user_id FROM contest_submissions
          WHERE contest_id = $1 AND shortlisted",
    )
    .bind(contest.id)
    .fetch_all(db)
    .await?;

    for user_id in finalists {
        issue_contest_attestation(
            db,
            user_id,
            contest,
            "contest_finalist",
            &format!("Finaliste — {}", contest.title),
            &format!(
                "Retenu parmi les {} finalistes du concours « {} », organisé par \
                 {company}.",
                contest.shortlist_size, contest.title
            ),
        )
        .await?;
    }

    Ok(())
}

/// One attestation resting on a contest.
///
/// `ON CONFLICT DO NOTHING` against the partial unique index: judging twice
/// must not hand somebody two identical proofs, which would double every
/// count that reads them.
async fn issue_contest_attestation(
    db: &PgPool,
    user_id: Uuid,
    contest: &Contest,
    basis: &str,
    title: &str,
    description: &str,
) -> Result<(), AppError> {
    let code = crate::services::attestations::AttestationsService::generate_verification_code();

    sqlx::query(
        "INSERT INTO attestations
            (user_id, attestation_type, basis, contest_id, title, description,
             issued_by_type, issued_by_org_id, verification_code)
         VALUES ($1, 'artefact', $2, $3, $4, $5, 'partner_enterprise', $6, $7)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(basis)
    .bind(contest.id)
    .bind(title)
    .bind(description)
    .bind(contest.enterprise_id)
    .bind(&code)
    .execute(db)
    .await?;

    Ok(())
}

/// The company hires somebody off the shortlist.
///
/// Reuses the success fee machinery from the recruitment campaigns: the
/// guarantee, the pro-rated refund and the departure tracking are the same
/// arrangement out of a different door, so this points at the contest rather
/// than duplicating any of it.
pub async fn record_hire(
    db: &PgPool,
    contest_id: Uuid,
    talent_user_id: Uuid,
    annual_salary: BigDecimal,
    guarantee_days: i64,
) -> Result<Uuid, AppError> {
    let contest = by_id(db, contest_id).await?;

    if contest.kind != "recruiting" {
        return Err(AppError::Validation(
            "only a recruiting contest ends in a hire".into(),
        ));
    }
    if !annual_salary.is_positive() {
        return Err(AppError::Validation(
            "the declared salary has to be a figure — the fee is a share of it".into(),
        ));
    }

    let rate = contest
        .success_fee_percent
        .clone()
        .unwrap_or_else(|| BigDecimal::from(10));
    let fee = (&annual_salary * &rate / BigDecimal::from(100))
        .with_scale_round(2, bigdecimal::RoundingMode::HalfUp);

    let mut tx = db.begin().await?;

    let marked = sqlx::query(
        "UPDATE contest_submissions SET hired = TRUE
          WHERE contest_id = $1 AND talent_user_id = $2
            AND shortlisted AND interview_completed",
    )
    .bind(contest_id)
    .bind(talent_user_id)
    .execute(&mut *tx)
    .await?;

    if marked.rows_affected() == 0 {
        return Err(AppError::Validation(
            "that person is not a shortlisted entrant who has been interviewed. The \
             prize was an interview, and a hire recorded without one rests on nothing \
             anybody can point at."
                .into(),
        ));
    }

    let ends_at = chrono::Utc::now() + chrono::Duration::days(guarantee_days.max(1));
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO recruitment_success_fees
            (contest_id, enterprise_id, talent_user_id, hired_at,
             annual_salary_declared, currency, success_fee_percent,
             success_fee_amount, guarantee_ends_at)
         VALUES ($1,$2,$3,NOW(),$4,$5,$6,$7,$8)
         RETURNING id",
    )
    .bind(contest_id)
    .bind(contest.enterprise_id)
    .bind(talent_user_id)
    .bind(&annual_salary)
    .bind(&contest.currency)
    .bind(&rate)
    .bind(&fee)
    .bind(ends_at)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO platform_revenues
            (source, related_talent_id, related_enterprise_id, amount_credits,
             fee_rate_bps, notes)
         VALUES ('recruitment_success_fee', $1, $2, $3, $4, $5)",
    )
    .bind(talent_user_id)
    .bind(contest.enterprise_id)
    .bind(&fee)
    .bind(crate::services::ledger::percent_to_bps(&rate))
    .bind(format!("concours {}", contest.slug))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // After the commit: an attestation that failed to write leaves a hire and
    // a fee that are both correct and repairable, where the other order would
    // leave a proof of a hire that was never recorded.
    issue_contest_attestation(
        db,
        talent_user_id,
        &contest,
        "contest_hired",
        &format!("Recruté — {}", contest.title),
        &format!(
            "Recruté à l'issue du concours « {} », après entretien.",
            contest.title
        ),
    )
    .await?;

    Ok(id)
}

/// Conclude a contest and book what Skilluv charged to run it.
pub async fn conclude(db: &PgPool, contest_id: Uuid) -> Result<BigDecimal, AppError> {
    let contest = by_id(db, contest_id).await?;
    if contest.status == "concluded" {
        return Err(AppError::Validation("already concluded".into()));
    }

    let unjudged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM contest_submissions
          WHERE contest_id = $1 AND judged_at IS NULL",
    )
    .bind(contest_id)
    .fetch_one(db)
    .await?;
    if unjudged > 0 {
        return Err(AppError::Validation(format!(
            "{unjudged} entries have no verdict. Concluding now would leave people who \
             spent days on this with nothing to show for it — not even a rank."
        )));
    }

    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE enterprise_contests
            SET status = 'concluded', concluded_at = NOW() WHERE id = $1",
    )
    .bind(contest_id)
    .execute(&mut *tx)
    .await?;

    if contest.orchestration_fee.is_positive() {
        sqlx::query(
            "INSERT INTO platform_revenues
                (source, related_enterprise_id, amount_credits, fee_rate_bps, notes)
             VALUES ($1, $2, $3, 10000, $4)",
        )
        .bind(if contest.kind == "recruiting" {
            "recruiting_contest_fee"
        } else {
            "contest_orchestration_fee"
        })
        .bind(contest.enterprise_id)
        .bind(&contest.orchestration_fee)
        .bind(format!("concours {} ({})", contest.slug, contest.kind))
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(contest.orchestration_fee)
}

/// Point a concluded contest at the work it turned into.
pub async fn set_outcome(
    db: &PgPool,
    contest_id: Uuid,
    engagement_id: Uuid,
) -> Result<(), AppError> {
    let contest = by_id(db, contest_id).await?;
    if contest.kind == "recruiting" {
        return Err(AppError::Validation(
            "a recruiting contest ends in a hire, not in an engagement".into(),
        ));
    }

    sqlx::query("UPDATE enterprise_contests SET outcome_engagement_id = $2 WHERE id = $1")
        .bind(contest_id)
        .bind(engagement_id)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn self_serve_charges_setup_plus_each_contact() {
        assert_eq!(
            contest_cost("self_serve", &dec("500"), &dec("20"), 3, &dec("0")),
            dec("560")
        );
    }

    #[test]
    fn managed_is_a_flat_fee_and_does_not_charge_per_contact() {
        // Skilluv does the sourcing; charging per contact on top would bill
        // twice for the same work.
        assert_eq!(
            contest_cost("managed", &dec("500"), &dec("20"), 10, &dec("3000")),
            dec("3000")
        );
    }

    #[test]
    fn a_contest_with_no_contacts_still_costs_the_setup() {
        assert_eq!(
            contest_cost("self_serve", &dec("500"), &dec("20"), 0, &dec("0")),
            dec("500")
        );
    }

    #[test]
    fn a_prize_pool_divides_without_losing_a_centime() {
        for (first, pool, winners) in [
            ("5000", "10000", 3),
            ("1000.00", "1000.00", 1),
            ("100", "1000", 7),
            ("3333.33", "9999.99", 4),
        ] {
            let prizes = prize_split(&dec(first), &dec(pool), winners);
            let total: BigDecimal = prizes.iter().sum();
            assert_eq!(
                total,
                dec(pool),
                "{pool} split {winners} ways lost or invented a centime"
            );
            assert_eq!(prizes[0], dec(first));
        }
    }

    #[test]
    fn a_pool_no_bigger_than_the_first_prize_pays_one_winner() {
        let prizes = prize_split(&dec("5000"), &dec("5000"), 3);
        assert_eq!(prizes, vec![dec("5000")]);
    }

    #[test]
    fn no_winners_is_no_prizes_rather_than_a_panic() {
        assert!(prize_split(&dec("5000"), &dec("10000"), 0).is_empty());
    }

    #[test]
    fn every_kind_and_mode_is_a_known_one() {
        assert_eq!(KINDS.len(), 5);
        assert!(KINDS.contains(&"migration"));
        assert_eq!(MODES.len(), 2);
    }
}
