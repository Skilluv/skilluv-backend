# Post-MVP backlog — enhancements & product moats

**Statut au 2026-07-15**

Backend architecturalement complet (25 phases livrées). Ce doc liste les
**enhancements produit** identifiés qui ne sont **pas bloquants pour un
MVP / lancement bêta**, mais qui peuvent transformer Skilluv d'un
"backend complet" en **plateforme différenciante** post-lancement.

**Chaque item est déjà scoppé pour être livrable en ≤ 1 sprint (5-10j)** — pas
de refactor architectural nécessaire, tout branche sur les 3 axes user
existants (skills / orientations / capabilities).

## Comment lire ce backlog

- **Tier 1 — Quick wins engagement** : petits ajouts, gros retour engagement.
  Idéal M+1 à M+3 post-lancement pour prouver l'itération produit.
- **Tier 2 — Product moats** : effort moyen, transforme un utilisateur "de
  passage" en "engagé long-terme". Cible M+3 à M+9.
- **Tier 3 — Ambitious** : nécessitent design produit + parfois DB
  refactor. Cible M+9 à M+18, dépend d'analytics utilisateurs.

Chaque entrée porte un **contract testable** : "après avoir livré, un
utilisateur peut faire X et le système démontre Y".

---

## Tier 1 — Quick wins engagement

### 1.1 Bookmarks / favoris

**Effort** : 2-3 jours. **Valeur** : ⭐⭐⭐

**Contract** : un user peut sauvegarder challenges, projets, users
(mentors), teams, deliverables inspirants — accès via `/api/users/me/bookmarks`.

**Design** :
- Table polymorphe `bookmarks(user_id, target_type, target_id, folder_slug, notes, created_at)`.
- `target_type` ∈ `{challenge_template, project, user, team, deliverable, bounty}`.
- Optional `folder_slug` pour organiser ("game-dev-projects", "mentors-frontend").
- Routes : `POST /bookmarks`, `DELETE /bookmarks/{id}`, `GET /users/me/bookmarks?target_type=X`.

**Prérequis** : rien.

**Pourquoi ça compte** : sans bookmark, un user qui découvre 5 challenges
intéressants pendant sa session doit les mémoriser mentalement. Résultat :
il n'y revient pas. Bookmarks = rétention.

---

### 1.2 Notes privées sur artefacts vus

**Effort** : 1-2 jours. **Valeur** : ⭐⭐

**Contract** : sur n'importe quel deliverable public consulté, un user
peut ajouter une note privée ("j'ai aimé cette approche", "revoir plus
tard").

**Design** :
- Table `user_notes(user_id, target_type, target_id, body TEXT, updated_at)`.
- Polymorphe même famille que bookmarks.
- Route `PUT /api/users/me/notes/{target_type}/{target_id}`.

**Prérequis** : rien.

**Pourquoi ça compte** : encourage la lecture active des artefacts
publics d'autres users → apprentissage social passif, valorise
implicitement les deliverables bien faits.

---

### 1.3 Objectifs personnels trackés

**Effort** : 3-4 jours. **Valeur** : ⭐⭐⭐

**Contract** : un user peut se fixer des objectifs mesurables
("atteindre Ranger d'ici 3 mois", "prouver React à niveau 4",
"terminer 5 challenges game-programmer") et voir sa progression en %.

**Design** :
- Table `user_goals(id, user_id, kind ∈ {rank, skill_level, capability, artifact_count}, target_value, deadline, created_at, achieved_at)`.
- Service `goals::compute_progress(user_id, goal_id)` qui lit l'état
  actuel (rank courant, skill.proficiency_level, capability active,
  count deliverables verified) et calcule un %.
- Route `GET /api/users/me/goals` avec `{goal, progress_pct,
  eta_days_at_current_pace}` calculé.

**Prérequis** : rien.

**Pourquoi ça compte** : transforme "je m'inscris pour voir" en "je
m'inscris pour atteindre X". Signal engagement +50% observé sur
plateformes similaires (Duolingo streak-goals, Strava challenges).

---

### 1.4 Historique d'apprentissage — timeline visuelle

**Effort** : 2-3 jours (backend), + design frontend. **Valeur** : ⭐⭐

**Contract** : sur son profil, un user voit sa timeline chronologique :
"2024-03 : rejoint Skilluv → 2024-06 : premier deliverable verified →
2024-09 : Ranger → 2025-02 : 1re attestation → 2025-08 : Artisan".

**Design** :
- Vue matérialisée `user_timeline_events(user_id, event_type, event_at, metadata JSONB)`.
- Événements : `signup`, `orientation_added`, `deliverable_verified`,
  `rank_promoted`, `capability_granted`, `attestation_received`,
  `event_participation`, `first_bounty_earned`, `first_mentor_session`.
- Populée en INSERT depuis les hooks P19.
- Route `GET /api/users/{id}/timeline` (public si profile_active).

**Prérequis** : hooks P19 (déjà en place).

**Pourquoi ça compte** : profil = objet de contemplation. Un timeline
riche = signal qualité pour recruteurs + fierté user. Alternative
lisible au portfolio statique.

---

## Tier 2 — Product moats

### 2.1 Cohorts / groupes d'étude temporaires

**Effort** : 6-8 jours. **Valeur** : ⭐⭐⭐⭐

**Contract** : un user peut créer ou rejoindre une **cohort** (temporaire,
5-30 personnes) autour d'un thème ("Rust bootcamp Q3 2026", "Reconversion
pentest sept-nov"). Chat de groupe, playlist partagée, deadlines
collectives.

**Design** :
- Table `cohorts(id, slug, name, description, starts_at, ends_at,
  max_members, orientation_slug, created_by, is_public)`.
- `cohort_members(cohort_id, user_id, joined_at, role IN
  ('member','organizer'))`.
- `cohort_milestones(cohort_id, title, target_date, description)` —
  livrables collectifs.
- Chat de groupe : réutilise `dm` étendu à group_dm (`group_id` nullable
  sur messages).
- Différent des **teams** (livraison d'un artefact commun) et **guilds**
  (identité communautaire long-terme). Une cohort = **cycle
  d'apprentissage borné dans le temps**.
- Routes : `POST /cohorts`, `POST /cohorts/{id}/join`, `GET
  /cohorts?orientation=X`, chat via WS.

**Prérequis** : orientations (P16), dm service (existant).

**Pourquoi ça compte** : les meilleurs bootcamps monétisent sur la
promesse "cohort" (bloc du meme). Skilluv peut offrir ça gratuitement
via community, avec valeur ajoutée : compagnonnage par mentors dans la
cohort. Fait exploser la valeur perçue vs plateformes isolées.

---

### 2.2 Coaching pair-à-pair structuré

**Effort** : 5-7 jours. **Valeur** : ⭐⭐⭐

**Contract** : deux users du même rang / orientation similaire peuvent
se pair-programmer 1 session/semaine avec check-in structuré (semi-formal,
distinct des sessions mentor payantes qui sont senior→junior).

**Design** :
- Table `peer_matches(id, user_a, user_b, orientation_slug, matched_at,
  active, weekly_cadence)`.
- Matching algo : même orientation + même rang ±1 + timezone compatible
  + working_languages overlap.
- Table `peer_sessions(match_id, session_at, notes_a, notes_b,
  rating_a, rating_b, canceled)`.
- Route `POST /api/users/me/peer-matching/enroll` avec preferences,
  route `GET /peer-matching/proposals` retourne 3 matches candidats.
- Différent des **mentor_sessions** (payantes, senior→junior, formal) :
  gratuit, peer-to-peer, informel.

**Prérequis** : orientations + rank + working_languages (P16.2 en place).

**Pourquoi ça compte** : le wedge autodidactes/reconversion a besoin
de camarades du même niveau, pas juste de mentors. Résout la solitude
de l'apprentissage à distance. Différenciateur fort vs LinkedIn
Learning / Coursera qui sont isolés.

---

### 2.3 Réputation cross-plateforme (import contrôlé)

**Effort** : 8-10 jours. **Valeur** : ⭐⭐⭐ (avec risque)

**Contract** : un user peut lier son GitHub / Medium / talks passés à
son profil Skilluv comme **signaux externes** — visibles mais
**clairement distincts** des preuves Skilluv (immuables).

**Design** :
- Table `external_signals(user_id, provider ∈ {github, medium, dev_to,
  conf_ref}, url, verified_at, meta JSONB)`.
- Vérification légère : OAuth pour GitHub (déjà connecté), OG scraping
  + user confirmation pour blogs.
- **Rendu UI distinct** : "Preuves Skilluv" (badges P17 immuables) vs
  "Signaux externes" (soft evidence, verifiable manually).
- **NE PAS** compter vers `weighted_proven_count` ou rank promotion —
  la règle "prouvé sur Skilluv" reste sacrée.

**Prérequis** : GitHub OAuth (existant), design UX pour distinguer les
2 catégories sans confondre.

**Pourquoi ça compte** : un dev qui a 5 ans XP GitHub arrive vide sur
Skilluv = frustrant + freine l'adoption. Import contrôlé accélère
l'onboarding sans diluer la vérifiabilité des preuves Skilluv.

**Risques à gérer** :
- Ne pas laisser un user "importer 500 stars GitHub" et se retrouver
  Doyen d'entrée. Signaux externes = affichage, pas rank.
- Provenance légale des données scrappées (Medium public OK, mais
  attention aux terms).

---

### 2.4 Notifications interactives sur promotion

**Effort** : 3-4 jours. **Valeur** : ⭐⭐⭐

**Contract** : quand un user est promu (rank, capability, badge), il
reçoit une notif riche avec CTA. Ex : "Tu viens d'être promu Ranger !
Voici les 3 projets débloqués à ton nouveau niveau."

**Design** :
- Extension `NotificationService::send` avec `notification_type` typés :
  `rank_promotion`, `capability_granted`, `badge_awarded`,
  `first_verified_deliverable`, `milestone_reached`.
- Template message enrichi par type (unlock_hint, next_step_cta).
- Wire dans `proof_hooks::recompute_all_for_user` (P19) : quand
  `promoted: true` → `NotificationService::send(...)`.

**Prérequis** : proof_hooks (P19), déjà en place.

**Pourquoi ça compte** : engager le user au moment psychologique clé
(dopamine promotion). Sans ça, un user peut atteindre Ranger sans le
savoir → pas de célébration = pas d'attachement.

---

## Tier 3 — Ambitious (product design nécessaire avant)

### 3.1 IA compagnon d'apprentissage disclosed

**Effort** : 3-4 semaines. **Valeur** : ⭐⭐⭐⭐ mais risqué

**Contract** : un user peut demander à l'IA Skilluv (via
`skilluv-ia` gRPC) : "explique-moi cette PR review", "génère 3
exercices sur ce skill", "revois mon code avant que je le soumette
en review" — le tout **loggé** dans `deliverables.verification_signal`
si code final soumis.

**Design** :
- Nouvelle route `POST /api/ai/companion/ask` avec type ∈ `{explain,
  generate_exercises, pre_review, debug_help}`.
- Rate limit strict par user (10 req/jour) pour maîtriser coûts.
- Toute utilisation IA loggée dans `ai_interactions` avec disclosure
  automatique sur le prochain deliverable soumis (`ai_policy` P8).
- Cache aggressif : mêmes questions → mêmes réponses.

**Prérequis** :
- skilluv-ia (Python) opérationnel prod (aujourd'hui stub P15.2).
- Politique tarifaire IA (coûts LLM en $ à contrôler).
- Cadre légal disclosure ("cet exercice a été fait avec assistance IA").

**Pourquoi ça compte** : ChatGPT est le concurrent invisible. Si
Skilluv n'offre pas d'IA disclosed, les users vont là-bas et cachent.
Autant l'intégrer proprement.

**Risques** :
- Dépendance forte à skilluv-ia (Python service).
- Coûts LLM peuvent exploser.
- Ligne fine entre "aide" et "triche" — dépend du cadre disclosure.

---

### 3.2 Marketplace inversé — talents proposent leur temps

**Effort** : 2-3 semaines. **Valeur** : ⭐⭐⭐

**Contract** : un talent (rank ≥ Artisan) peut afficher "je propose
2h/semaine pour du pair-programming Rust senior" — visible aux users
qui cherchent ce skill, gratuit ou payant selon config.

**Design** :
- Table `talent_offers(user_id, offer_type, skill_id, availability_hours,
  price_cents_per_hour NULLABLE, description)`.
- `offer_type` ∈ `{pair_programming, code_review, whiteboard,
  mock_interview, career_advice}`.
- Différent des `mentor_sessions` (bookings 1-1 formels via mentor_profile)
  : plus léger, éphémère, self-serve.

**Prérequis** : Rank system (P17.4), Stripe Connect (P13.2).

**Pourquoi ça compte** : inverse la marketplace mentor (enterprise
cherche talent) → talent propose sa dispo. Signale l'expertise, crée
du revenu passif pour les Artisans+.

---

### 3.3 Reputation staking / vouching

**Effort** : 3-4 semaines. **Valeur** : ⭐⭐

**Contract** : un user Doyen peut "vouch" pour un junior — mettre sa
propre rank en jeu ("si ce user commet une fraude dans les 6 mois, je
perds une rank temporairement"). Vouching visible = accélère la
crédibilité du junior.

**Design** :
- Table `vouchings(voucher_id, vouched_id, active_until, at_stake_kind,
  broken_at, break_reason)`.
- Web-of-trust léger sans blockchain.
- Impact sur talent_search : "vouched by X mentors" boost score.

**Prérequis** : mentor capability P18, rank system P17.4, admin
process anti-abus.

**Pourquoi ça compte** : résout le cold-start problem des juniors —
comment un utilisateur zéro-preuve peut-il être crédible ? Solution :
un senior met sa peau au jeu. Signal fort pour recruteurs.

**Risques** : complexité produit, potentiel abus si mal cadré.

---

### 3.4 Skill tree / dependency graph visuel

**Effort** : 2-3 semaines (backend + frontend heavy). **Valeur** : ⭐⭐

**Contract** : le user voit un **arbre visuel de ses skills**, avec
dépendances (`react` requires `javascript`, `godot-3d` requires
`blender-basics`). Progression rendue comme un arbre RPG.

**Design** :
- La `skill_nodes.parent_id` existe déjà (P4). Ajout `prerequisite_skill_ids UUID[]`.
- Route `GET /api/users/{id}/skill-tree` avec structure hiérarchique
  + statut par nœud (unlocked / in_progress / locked).
- Frontend heavy pour rendu D3.js ou similaire.

**Prérequis** : DB extension légère, gros travail frontend.

**Pourquoi ça compte** : gamification puissante, mais **danger de
tomber dans le WoW-UI kitsch** (voir spec BMAD garde-fous). À faire
uniquement si l'esthétique reste editoriale (Persona-inspired, pas
Diablo).

---

## Ce qu'on ne fera JAMAIS (par design produit)

- **Certifications payantes** — contredit "talents ne payent pas".
- **NFT / crypto attestations** — attestations Skilluv sont
  vérifiables cryptographiquement mais restent hors-chain.
- **Sponsored content dans le feed** — le feed est piloté par
  pertinence, pas par $$.
- **AI-only auto-grading** sans peer/mentor review — contredit
  "human_verified > auto".
- **Multi-account allowed** — un user = une identité (anti-fraude
  P14.4).

---

## Critères de sélection MVP → v1.1

Après lancement, pour choisir quoi implémenter :

1. **Signaux user analytics** : quel comportement les users cherchent-ils
   et ne trouvent pas ? (heatmap des clics vides, questions support).
2. **Rétention D7 / D30** : quel enhancement pousse-t-il ces métriques ?
3. **Feedback direct** via `contact_requests` catégorie feature_request.

**Reco de séquence** post-lancement :
1. **M+1 → M+3** : Tier 1 complet (bookmarks + notes + goals + timeline
   = ~2 semaines de dev cumulé). Retour engagement immédiat.
2. **M+3 → M+6** : Tier 2 items choisis selon analytics (cohorts
   probable si signal "solitude" fort, peer coaching si demande junior).
3. **M+6 → M+12** : Tier 3 discuté avec la communauté existante (RFC
   process, décision produit informée).

---

## Lien avec l'existant

- Tier 1.3 (goals) réutilise `capabilities_engine` + `ranks` (P17-P18).
- Tier 1.4 (timeline) réutilise les hooks `proof_hooks` (P19).
- Tier 2.1 (cohorts) réutilise `dm` + orientations (P16).
- Tier 2.2 (peer coaching) réutilise `orientations` + `rank` + `working_languages`.
- Tier 3.1 (AI companion) réutilise `skilluv-ia` gRPC client (P15.2).

**Aucun de ces items ne nécessite un refactor architectural du backend
actuel.** Le socle est solide, ces enhancements branchent proprement.
