use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::AppState;

pub fn openapi_routes() -> Router<AppState> {
    Router::new().route("/docs/openapi.json", get(openapi_spec))
}

async fn openapi_spec() -> Json<serde_json::Value> {
    Json(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Skilluv API",
            "description": "Plateforme gamifiée de démonstration de compétences",
            "version": "1.0.0",
            "contact": {
                "name": "Skilluv Team",
                "url": "https://skilluv.com"
            }
        },
        "servers": [
            { "url": "http://localhost:3001/api", "description": "Development" }
        ],
        "tags": [
            { "name": "Auth", "description": "Authentication & account management" },
            { "name": "Profile", "description": "User profile management" },
            { "name": "Challenges", "description": "Challenge catalog & submissions" },
            { "name": "Gamification", "description": "Skill tree, heatmap, badges" },
            { "name": "Leaderboard", "description": "Rankings & leaderboards" },
            { "name": "Sandbox", "description": "Code execution via Judge0" },
            { "name": "Enterprise", "description": "Enterprise accounts & recruitment" },
            { "name": "Contact", "description": "Interest requests & messaging" },
            { "name": "Community", "description": "Community challenges & voting" },
            { "name": "Notifications", "description": "User notifications" },
            { "name": "Developer", "description": "API keys & webhooks" },
            { "name": "Admin", "description": "Admin dashboard & moderation" },
            { "name": "Public API", "description": "Public v1 API (API key auth)" },
        ],
        "paths": {
            // ─── Auth ───
            "/auth/register": { "post": { "tags": ["Auth"], "summary": "Register a new user", "security": [] }},
            "/auth/login": { "post": { "tags": ["Auth"], "summary": "Login with email or username", "security": [] }},
            "/auth/refresh": { "post": { "tags": ["Auth"], "summary": "Refresh access token" }},
            "/auth/logout": { "post": { "tags": ["Auth"], "summary": "Logout (revoke tokens)" }},
            "/auth/me": { "get": { "tags": ["Auth"], "summary": "Get current user with rank" }},
            "/auth/verify-email": { "get": { "tags": ["Auth"], "summary": "Verify email via token", "security": [] }},
            "/auth/resend-verification": { "post": { "tags": ["Auth"], "summary": "Resend verification email" }},
            "/auth/forgot-password": { "post": { "tags": ["Auth"], "summary": "Request password reset", "security": [] }},
            "/auth/reset-password": { "post": { "tags": ["Auth"], "summary": "Reset password with token", "security": [] }},
            "/auth/change-password": { "post": { "tags": ["Auth"], "summary": "Change password (authenticated)" }},
            "/auth/totp/setup": { "post": { "tags": ["Auth"], "summary": "Setup TOTP 2FA" }},
            "/auth/totp/enable": { "post": { "tags": ["Auth"], "summary": "Enable TOTP 2FA" }},
            "/auth/totp/disable": { "post": { "tags": ["Auth"], "summary": "Disable TOTP 2FA" }},
            "/auth/email-2fa/enable": { "post": { "tags": ["Auth"], "summary": "Enable email 2FA" }},
            "/auth/email-2fa/disable": { "post": { "tags": ["Auth"], "summary": "Disable email 2FA" }},
            "/auth/email-2fa/verify": { "post": { "tags": ["Auth"], "summary": "Verify email 2FA code" }},
            "/auth/account": { "delete": { "tags": ["Auth"], "summary": "Delete account (RGPD)" }},
            "/auth/me/display-name": { "put": { "tags": ["Profile"], "summary": "Update display name" }},
            "/auth/me/skill-domain": { "put": { "tags": ["Profile"], "summary": "Change skill domain" }},

            // ─── Profile ───
            "/profile/{username}": { "get": { "tags": ["Profile"], "summary": "Public profile (SSR-ready)", "security": [] }},
            "/profile/me": { "put": { "tags": ["Profile"], "summary": "Update bio, social links, country" }},
            "/profile/me/avatar": {
                "post": { "tags": ["Profile"], "summary": "Upload avatar (multipart, max 2MB)" },
                "delete": { "tags": ["Profile"], "summary": "Delete avatar" }
            },
            "/profile/me/privacy": {
                "get": { "tags": ["Profile"], "summary": "Get privacy settings" },
                "put": { "tags": ["Profile"], "summary": "Update privacy settings" }
            },

            // ─── Challenges ───
            "/challenges": { "get": { "tags": ["Challenges"], "summary": "List published challenges (filterable)", "security": [] }},
            "/challenges/onboarding": { "get": { "tags": ["Challenges"], "summary": "Get onboarding challenge for domain" }},
            "/challenges/tags": { "get": { "tags": ["Challenges"], "summary": "List all tags with counts", "security": [] }},
            "/challenges/categories": { "get": { "tags": ["Challenges"], "summary": "List tag categories", "security": [] }},
            "/challenges/featured": { "get": { "tags": ["Challenges"], "summary": "Featured challenges", "security": [] }},
            "/challenges/{id}": { "get": { "tags": ["Challenges"], "summary": "Get challenge details", "security": [] }},
            "/challenges/{id}/start": { "post": { "tags": ["Challenges"], "summary": "Start a challenge (creates submission)" }},
            "/challenges/{id}/submit": { "post": { "tags": ["Challenges"], "summary": "Submit solution" }},
            "/challenges/{id}/submissions": { "get": { "tags": ["Challenges"], "summary": "My submissions for a challenge" }},
            "/challenges/{id}/timer": { "get": { "tags": ["Challenges"], "summary": "Timer status for active submission" }},
            "/challenges/{id}/timer/extend": { "post": { "tags": ["Challenges", "Admin"], "summary": "Extend timer (admin)" }},
            "/challenges/{id}/teams": { "get": { "tags": ["Challenges"], "summary": "List teams for team challenge" }},
            "/challenges/{id}/team/create": { "post": { "tags": ["Challenges"], "summary": "Create a team" }},
            "/challenges/{id}/team/{team_id}/join": { "post": { "tags": ["Challenges"], "summary": "Join a team" }},
            "/challenges/{id}/team/{team_id}/submit": { "post": { "tags": ["Challenges"], "summary": "Submit as team" }},

            // ─── Community ───
            "/community/challenges": { "post": { "tags": ["Community"], "summary": "Submit community challenge" }},
            "/community/challenges/mine": { "get": { "tags": ["Community"], "summary": "My community challenges" }},
            "/community/challenges/{id}": { "put": { "tags": ["Community"], "summary": "Edit community challenge" }},
            "/community/challenges/{id}/vote": {
                "post": { "tags": ["Community"], "summary": "Upvote challenge" },
                "delete": { "tags": ["Community"], "summary": "Remove vote" }
            },
            "/community/challenges/popular": { "get": { "tags": ["Community"], "summary": "Popular community challenges", "security": [] }},

            // ─── Gamification ───
            "/skills/tree": { "get": { "tags": ["Gamification"], "summary": "My skill tree" }},
            "/skills/tree/{user_id}": { "get": { "tags": ["Gamification"], "summary": "User's skill tree" }},
            "/activity/heatmap": { "get": { "tags": ["Gamification"], "summary": "My activity heatmap (12 months)" }},
            "/activity/heatmap/{user_id}": { "get": { "tags": ["Gamification"], "summary": "User's activity heatmap" }},

            // ─── Leaderboard ───
            "/leaderboards": { "get": { "tags": ["Leaderboard"], "summary": "List available leaderboards", "security": [] }},
            "/leaderboards/{domain}": { "get": { "tags": ["Leaderboard"], "summary": "Get leaderboard (paginated)", "security": [] }},
            "/leaderboards/{domain}/me": { "get": { "tags": ["Leaderboard"], "summary": "My rank in domain" }},

            // ─── Sandbox ───
            "/sandbox/execute": { "post": { "tags": ["Sandbox"], "summary": "Execute code (sync)" }},
            "/sandbox/execute-async": { "post": { "tags": ["Sandbox"], "summary": "Execute code (async)" }},
            "/sandbox/result/{token}": { "get": { "tags": ["Sandbox"], "summary": "Get async execution result" }},
            "/sandbox/languages": { "get": { "tags": ["Sandbox"], "summary": "List supported languages" }},

            // ─── Enterprise ───
            "/enterprise/register": { "post": { "tags": ["Enterprise"], "summary": "Register enterprise account", "security": [] }},
            "/enterprise/profile": {
                "get": { "tags": ["Enterprise"], "summary": "Get enterprise profile" },
                "put": { "tags": ["Enterprise"], "summary": "Update enterprise profile" }
            },
            "/enterprise/invite": { "post": { "tags": ["Enterprise"], "summary": "Invite recruiter" }},
            "/enterprise/invite/accept": { "post": { "tags": ["Enterprise"], "summary": "Accept recruiter invite", "security": [] }},
            "/enterprise/members": { "get": { "tags": ["Enterprise"], "summary": "List team members" }},
            "/enterprise/members/{user_id}": { "delete": { "tags": ["Enterprise"], "summary": "Revoke member" }},
            "/enterprise/bookmarks/{talent_id}": {
                "post": { "tags": ["Enterprise"], "summary": "Bookmark talent" },
                "delete": { "tags": ["Enterprise"], "summary": "Remove bookmark" }
            },
            "/enterprise/bookmarks": { "get": { "tags": ["Enterprise"], "summary": "List bookmarks" }},
            "/enterprise/lists": {
                "post": { "tags": ["Enterprise"], "summary": "Create talent list" },
                "get": { "tags": ["Enterprise"], "summary": "List talent lists" }
            },
            "/enterprise/lists/{list_id}": {
                "get": { "tags": ["Enterprise"], "summary": "Get list with talents" },
                "put": { "tags": ["Enterprise"], "summary": "Update list" },
                "delete": { "tags": ["Enterprise"], "summary": "Delete list" }
            },
            "/enterprise/lists/{list_id}/talents/{talent_id}": {
                "post": { "tags": ["Enterprise"], "summary": "Add talent to list" },
                "delete": { "tags": ["Enterprise"], "summary": "Remove talent from list" }
            },
            "/enterprise/dashboard/platform-stats": { "get": { "tags": ["Enterprise"], "summary": "Platform stats" }},
            "/enterprise/dashboard/my-stats": { "get": { "tags": ["Enterprise"], "summary": "Enterprise stats" }},

            // ─── Talent Search ───
            "/talents/search": { "get": { "tags": ["Enterprise"], "summary": "Search talents (SSR-ready)", "security": [] }},
            "/talents/{username}/card": { "get": { "tags": ["Enterprise"], "summary": "Talent card", "security": [] }},

            // ─── Contact ───
            "/contact/interest": { "post": { "tags": ["Contact"], "summary": "Send interest request" }},
            "/contact/interest/sent": { "get": { "tags": ["Contact"], "summary": "Sent interest requests" }},
            "/contact/interest/received": { "get": { "tags": ["Contact"], "summary": "Received interest requests" }},
            "/contact/interest/{id}/accept": { "post": { "tags": ["Contact"], "summary": "Accept interest request" }},
            "/contact/interest/{id}/decline": { "post": { "tags": ["Contact"], "summary": "Decline interest request" }},
            "/contact/conversations": { "get": { "tags": ["Contact"], "summary": "List conversations" }},
            "/contact/conversations/{id}": { "get": { "tags": ["Contact"], "summary": "Get conversation messages" }},
            "/contact/conversations/{id}/messages": { "post": { "tags": ["Contact"], "summary": "Send message" }},
            "/contact/block/{enterprise_id}": {
                "post": { "tags": ["Contact"], "summary": "Block enterprise" },
                "delete": { "tags": ["Contact"], "summary": "Unblock enterprise" }
            },

            // ─── Notifications ───
            "/notifications": { "get": { "tags": ["Notifications"], "summary": "List notifications" }},
            "/notifications/{id}/read": { "post": { "tags": ["Notifications"], "summary": "Mark as read" }},
            "/notifications/read-all": { "post": { "tags": ["Notifications"], "summary": "Mark all as read" }},
            "/notifications/unread-count": { "get": { "tags": ["Notifications"], "summary": "Unread count" }},

            // ─── Reports ───
            "/reports": { "post": { "tags": ["Admin"], "summary": "Submit a report" }},
            "/reports/mine": { "get": { "tags": ["Admin"], "summary": "My reports" }},
            "/reports/{id}": { "delete": { "tags": ["Admin"], "summary": "Cancel report" }},

            // ─── Developer ───
            "/developer/keys": {
                "post": { "tags": ["Developer"], "summary": "Create API key" },
                "get": { "tags": ["Developer"], "summary": "List API keys" }
            },
            "/developer/keys/{id}": { "delete": { "tags": ["Developer"], "summary": "Revoke key" }},
            "/developer/keys/{id}/regenerate": { "post": { "tags": ["Developer"], "summary": "Regenerate key" }},
            "/developer/keys/{id}/usage": { "get": { "tags": ["Developer"], "summary": "Key usage stats" }},
            "/developer/webhooks": {
                "post": { "tags": ["Developer"], "summary": "Create webhook" },
                "get": { "tags": ["Developer"], "summary": "List webhooks" }
            },
            "/developer/webhooks/{id}": {
                "put": { "tags": ["Developer"], "summary": "Update webhook" },
                "delete": { "tags": ["Developer"], "summary": "Delete webhook" }
            },
            "/developer/webhooks/{id}/test": { "post": { "tags": ["Developer"], "summary": "Send test event" }},

            // ─── Public API v1 ───
            "/v1/users/{username}": { "get": { "tags": ["Public API"], "summary": "Get user profile (API key)", "security": [{"apiKey": []}] }},
            "/v1/users/{username}/badges": { "get": { "tags": ["Public API"], "summary": "Get user badges (API key)", "security": [{"apiKey": []}] }},
            "/v1/users/{username}/skills": { "get": { "tags": ["Public API"], "summary": "Get user skills (API key)", "security": [{"apiKey": []}] }},

            // ─── Admin ───
            "/admin/challenges": {
                "post": { "tags": ["Admin"], "summary": "Create challenge" },
                "get": { "tags": ["Admin"], "summary": "List all challenges" }
            },
            "/admin/challenges/{id}": { "put": { "tags": ["Admin"], "summary": "Update challenge" }},
            "/admin/challenges/{id}/publish": { "post": { "tags": ["Admin"], "summary": "Publish challenge" }},
            "/admin/challenges/{id}/archive": { "post": { "tags": ["Admin"], "summary": "Archive challenge" }},
            "/admin/stats": { "get": { "tags": ["Admin"], "summary": "Platform stats" }},
            "/admin/leaderboards/rebuild": { "post": { "tags": ["Admin"], "summary": "Rebuild leaderboards from DB" }},
            "/admin/users": { "get": { "tags": ["Admin"], "summary": "List users (filterable)" }},
            "/admin/users/{id}": { "get": { "tags": ["Admin"], "summary": "User detail" }},
            "/admin/users/{id}/ban": { "post": { "tags": ["Admin"], "summary": "Ban user" }},
            "/admin/users/{id}/unban": { "post": { "tags": ["Admin"], "summary": "Unban user" }},
            "/admin/reports": { "get": { "tags": ["Admin"], "summary": "List reports" }},
            "/admin/reports/{id}": { "put": { "tags": ["Admin"], "summary": "Handle report" }},
            "/admin/audit-log": { "get": { "tags": ["Admin"], "summary": "Audit log" }},
            "/admin/dashboard/moderation": { "get": { "tags": ["Admin"], "summary": "Moderation dashboard" }},
            "/admin/community/review": { "get": { "tags": ["Admin"], "summary": "Community challenges pending review" }},
            "/admin/community/{id}/approve": { "post": { "tags": ["Admin"], "summary": "Approve community challenge" }},
            "/admin/community/{id}/reject": { "post": { "tags": ["Admin"], "summary": "Reject community challenge" }},

            // ─── Health ───
            "/health": { "get": { "tags": ["Health"], "summary": "Service health check", "security": [] }},
        },
        "components": {
            "securitySchemes": {
                "cookieAuth": {
                    "type": "apiKey",
                    "in": "cookie",
                    "name": "access_token",
                    "description": "JWT access token in HttpOnly cookie"
                },
                "apiKey": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "API key (sk_live_xxx)"
                }
            }
        },
        "security": [{ "cookieAuth": [] }]
    }))
}
