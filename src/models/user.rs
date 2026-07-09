use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    /// NULL until the user picks their domain during onboarding (Pattern C: SSO signups start
    /// without one). Accessors below expose a lossless `&str` view for legacy call sites.
    pub skill_domain: Option<String>,
    pub role: String,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub streak_current: i32,
    pub streak_last_activity: Option<NaiveDate>,
    pub trust_score: f32,
    pub country: Option<String>,
    pub city: Option<String>,
    pub email_verified: bool,
    #[serde(skip_serializing)]
    pub totp_secret: Option<Vec<u8>>,
    pub totp_enabled: bool,
    pub email_2fa_enabled: bool,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub github: Option<String>,
    pub linkedin: Option<String>,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub profile_active: bool,
    pub is_banned: bool,
    pub ban_reason: Option<String>,
    pub banned_at: Option<DateTime<Utc>>,
    pub banned_by: Option<Uuid>,
    pub terms_accepted_at: Option<DateTime<Utc>>,
    pub password_changed_at: DateTime<Utc>,
    pub failed_login_count: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    pub skill_domain: Option<String>,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub streak_current: i32,
    pub trust_score: f32,
    pub country: Option<String>,
    pub city: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub github: Option<String>,
    pub linkedin: Option<String>,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub email_verified: bool,
    pub totp_enabled: bool,
    pub email_2fa_enabled: bool,
    pub profile_active: bool,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            username: u.username,
            first_name: u.first_name,
            last_name: u.last_name,
            display_name: u.display_name,
            skill_domain: u.skill_domain,
            title: u.title,
            golden_stars: u.golden_stars,
            total_fragments: u.total_fragments,
            streak_current: u.streak_current,
            trust_score: u.trust_score,
            country: u.country,
            city: u.city,
            bio: u.bio,
            avatar_url: u.avatar_url,
            github: u.github,
            linkedin: u.linkedin,
            website: u.website,
            twitter: u.twitter,
            email_verified: u.email_verified,
            totp_enabled: u.totp_enabled,
            email_2fa_enabled: u.email_2fa_enabled,
            profile_active: u.profile_active,
            created_at: u.created_at,
        }
    }
}

/// Données privées retournées uniquement au propriétaire du compte
#[derive(Debug, Serialize)]
pub struct UserPrivate {
    pub id: Uuid,
    pub email: String,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub display_name: String,
    /// Global role — 'user', 'recruiter', 'enterprise', or 'admin'. The
    /// frontend uses it to gate the enterprise layout (mandatory-TOTP) and to
    /// pick the right nav shell.
    pub role: String,
    pub skill_domain: Option<String>,
    /// True once the user has picked a skill_domain **and** accepted the terms.
    /// The frontend uses this to decide whether to force the onboarding step.
    pub profile_completed: bool,
    pub title: String,
    pub golden_stars: i32,
    pub total_fragments: i32,
    pub streak_current: i32,
    pub trust_score: f32,
    pub country: Option<String>,
    pub city: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub github: Option<String>,
    pub linkedin: Option<String>,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub email_verified: bool,
    pub totp_enabled: bool,
    pub email_2fa_enabled: bool,
    pub profile_active: bool,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserPrivate {
    fn from(u: User) -> Self {
        let profile_completed = u.skill_domain.is_some() && u.terms_accepted_at.is_some();
        Self {
            id: u.id,
            email: u.email,
            username: u.username,
            first_name: u.first_name,
            last_name: u.last_name,
            display_name: u.display_name,
            role: u.role,
            skill_domain: u.skill_domain,
            profile_completed,
            title: u.title,
            golden_stars: u.golden_stars,
            total_fragments: u.total_fragments,
            streak_current: u.streak_current,
            trust_score: u.trust_score,
            country: u.country,
            city: u.city,
            bio: u.bio,
            avatar_url: u.avatar_url,
            github: u.github,
            linkedin: u.linkedin,
            website: u.website,
            twitter: u.twitter,
            email_verified: u.email_verified,
            totp_enabled: u.totp_enabled,
            email_2fa_enabled: u.email_2fa_enabled,
            profile_active: u.profile_active,
            created_at: u.created_at,
        }
    }
}
