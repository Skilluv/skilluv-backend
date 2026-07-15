# Modération front vs Admin panel — split de responsabilités

**Statut au 2026-07-15** : P25 livre les 5 capabilities community moderator +
helpers `require_capability` / `require_any_capability`. Ce doc explique
comment le front doit consommer.

## Principe fondateur

Deux fronts distincts sur Skilluv :
- **`skilluv-frontend`** — l'app utilisateur (Svelte), où vivent talents,
  mentors, entreprises, ET **modérateurs communautaires** (power users
  volontaires de confiance).
- **`skilluv-admin`** — le back-office staff Skilluv (réservé aux
  salariés / core team).

**La règle** : un modérateur communautaire n'a **jamais** accès à
`skilluv-admin`. Ses outils vivent inline dans `skilluv-frontend`,
apparaissant conditionnellement selon ses capabilities.

## Mapping capability → surface autorisée

| Capability | Front autorisé | Rôle |
|---|---|---|
| `challenger` | skilluv-frontend | Default, tout user |
| `mentor` | skilluv-frontend | Attester + sessions payantes |
| `project_steward` | skilluv-frontend | Owner projet OSS |
| `pr_reviewer` | skilluv-frontend | Review PR |
| `bounty_funder` | skilluv-frontend | (via son enterprise) |
| `issue_proposer` | skilluv-frontend | Proposer challenges |
| `jury_tournament` | skilluv-frontend | Juger tournoi |
| `enterprise_recruiter` | skilluv-frontend | Talent search |
| **`community_moderator`** | **skilluv-frontend** | Umbrella meta-cap |
| **`forum_moderator`** | **skilluv-frontend** | Delete post, mute user 24h |
| **`plagiarism_reviewer`** | **skilluv-frontend** | Décision plagiat (mark_valid / revoke) |
| **`kyc_reviewer`** | **skilluv-frontend** | Approve/deny KYC Momo > 100k XOF |
| **`community_curator`** | **skilluv-frontend** | Approve/reject challenges community |
| **`admin`** | **skilluv-admin ONLY** | Staff Skilluv, accès système complet |

## Ce qui n'est PAS accessible aux moderators (réservé admin)

- Voir tous les users + CRUD (dont bannissement définitif).
- Financials / revenue metrics dashboard.
- Grant / revoke capabilities (aucun moderator ne peut promouvoir un autre).
- Modifier `badge_rules`, `orientations`, seed catalogues.
- Config providers (FCM keys, Stripe secrets, tenant limits).
- Audit logs complets, exports GDPR.
- Feature flags système.
- RLS enforcement toggle.
- Investigations volumétriques cross-tenant.

Ces actions restent gated par `require_capability("admin")` et servies
uniquement depuis les routes `/api/admin/**`.

## Exemples de wiring backend attendu (P26+ à implémenter)

### Modération inline — accessible depuis front

```rust
// POST /api/forum/posts/{id}/moderate — accessible forum_mod ou admin
async fn moderate_post(State(state), auth: AuthUser, ...) {
    require_any_capability(&state.db, auth.user_id,
        &["forum_moderator", "admin"]).await?;
    // ... action de modération
}

// POST /api/fraud/deliverables/{id}/mark-valid — plagiarism_reviewer ou admin
async fn mark_valid(State(state), auth: AuthUser, ...) {
    require_any_capability(&state.db, auth.user_id,
        &["plagiarism_reviewer", "admin"]).await?;
    // ...
}

// POST /api/community/challenges/{id}/approve — curator ou admin
async fn approve_community_challenge(State(state), auth, ...) {
    require_any_capability(&state.db, auth.user_id,
        &["community_curator", "admin"]).await?;
    // ...
}
```

### Admin uniquement — servi depuis skilluv-admin

```rust
// POST /api/admin/users/{id}/capabilities — grant capability
async fn admin_grant(...) {
    require_capability(&state.db, auth.user_id, "admin").await?;
    // pas d'alternative moderator
}
```

## Comment le front conditionne l'UI

`GET /api/users/me/capabilities` retourne la liste actives. Le front
Svelte peut ensuite :

```svelte
{#if $me.capabilities.includes('forum_moderator') || $me.capabilities.includes('admin')}
  <button on:click={openModerationDrawer}>🛡️ Modérer</button>
{/if}
```

Aucun endpoint système n'apparaît si la cap admin n'est pas active.

## Analogies éprouvées

- **GitHub** : mainteneurs de repos modèrent leurs projets sur
  github.com. Staff GitHub utilise des outils internes séparés.
- **Reddit** : modos de subreddits utilisent reddit.com avec boutons
  mod. Admins Reddit ont un panneau séparé.
- **Discord** : modos de serveurs utilisent Discord classique. Trust
  & Safety Discord = tooling interne.

C'est exactement le pattern Skilluv.

## Points d'attention sécurité

1. **Jamais confier "admin" à un moderator, même temporairement.** Si
   un moderator a besoin d'une action système one-off (débloquer un
   compte suspect), il passe par un admin qui exécute.
2. **Rate-limit les actions modération** (rate_limit middleware
   existant à étendre) pour prévenir un moderator compromis qui
   spammerait des `mute/revoke`.
3. **Audit trail toutes les actions modération** (déjà en place via
   `audit_logs` P4). Chaque `mute/revoke/approve` doit tracer le
   moderator + timestamp + raison.
4. **Notification à l'utilisateur affecté** — si un post est deleted
   ou un deliverable revoked par un moderator, l'utilisateur reçoit
   une notif expliquant pourquoi (transparence).

## Migration existante

Les 5 endpoints admin fraud (`P14.5`) qui utilisaient `require_admin`
sont candidats à l'ouverture aux `plagiarism_reviewer` :
- `POST /admin/fraud/deliverables/{id}/mark-valid`
- `POST /admin/fraud/deliverables/{id}/revoke`
- `POST /admin/fraud/users/{id}/mark-valid`

À réviser en P26+ selon la stratégie produit.
