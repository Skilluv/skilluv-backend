# What you may attack, and what you may not

**English below the French. / Version française d'abord.**

This page is the scope of Skilluv's disclosure programme. It is the document
`SECURITY.md` points at, and it is served in machine-readable form at
`GET /api/security/scope` — unauthenticated, on purpose, because a researcher
decides what to touch before they have an account.

The list of hosts here is a copy. **The authoritative one is
`DEFAULT_SCOPE_HOSTS` in `src/services/security_findings.rs`**, which is what
refuses a submission. If the two ever disagree, the code is right and this page
is out of date — tell us.

---

## Ce que vous pouvez attaquer

### Dans le périmètre

| Hôte | Ce que c'est | Remarque |
|---|---|---|
| `staging.skill-uv.com` | Préproduction | **La cible à privilégier.** Données factices, remise à zéro chaque nuit. |
| `api.skill-uv.com` | API de production | Compte de test uniquement. Pas de données d'autrui. |
| `skill-uv.com` | Front public | |
| `admin.skill-uv.com` | Panneau d'administration | La page de connexion est dans le périmètre ; une session admin obtenue est une découverte critique à signaler immédiatement. |
| `ctf.skill-uv.com` | Terrain d'entraînement hébergé | Volontairement vulnérable. Rien de ce qu'on y trouve n'est une découverte. |

Le code source des quatre dépôts publics est également dans le périmètre pour
la lecture : `skilluv-backend`, `skilluv-frontend`, `skilluv-admin`,
`skilluv-ia`. Lire le code ne demande aucune autorisation. Tester le service
déployé relève de cette page.

### Hors périmètre

- `docs.skill-uv.com` — statique, hébergé par un tiers.
- Le serveur Discord et tout ce qui y est lié.
- Les comptes et l'infrastructure de tiers : Brevo, Stripe, GitHub, Cloudflare,
  Coolify, Hetzner. Signalez-leur directement.
- Tout hôte qui n'est pas dans le tableau ci-dessus, même s'il nous appartient
  manifestement. Un sous-domaine oublié est une découverte à signaler, pas une
  invitation.

### Vulnérabilités recherchées

Injection (SQL, NoSQL, commande, template), XSS sous ses trois formes, CSRF,
IDOR et contournement d'autorisation, contournement d'authentification, SSRF,
XXE, traversée de chemin, désérialisation, exécution de code à distance,
conditions de course, défauts de logique métier, faiblesses cryptographiques,
divulgation d'information par l'API ou les journaux, contournement de limite de
débit (à documenter, pas à exploiter).

### Vulnérabilités hors périmètre

- **Tout déni de service.** Y compris les tests de charge, y compris « juste
  pour voir combien ça tient ». C'est la seule interdiction dont le
  non-respect met fin à la relation immédiatement.
- Force brute au-delà des limites publiées (voir le mode recherche ci-dessous).
- Ingénierie sociale, hameçonnage, appels aux utilisateurs ou à l'équipe.
- Attaques physiques.
- En-têtes de sécurité manquants, sans impact démontré. Signalez-les groupés
  comme « durcissement » — ils sont utiles et ils ne sont pas des
  vulnérabilités.
- Vulnérabilités dans une dépendance sans preuve d'atteignabilité ici. Une
  entrée d'avis dans l'arbre de dépendances n'est pas une découverte.
- Sortie brute d'un scanner. Un rapport qui ne contient rien qu'un outil
  n'aurait pas produit sera refusé.
- Ce qui est déjà au tableau d'honneur.

### Règles d'engagement

1. **N'extrayez pas plus que nécessaire** pour démontrer la faille. Une ligne
   suffit à prouver une lecture non autorisée ; mille lignes sont une fuite que
   vous avez causée.
2. **Ne persistez pas.** Pas de porte dérobée, pas de compte administrateur
   créé, rien laissé derrière.
3. **Ne divulguez pas** avant coordination. L'embargo par défaut est de 90
   jours à compter de la confirmation, et vous êtes crédité à la publication.
4. **Identifiez-vous.** Ajoutez l'en-tête `X-Security-Research: <votre
   identifiant Skilluv>` à vos requêtes. Cela ne change rien à vos droits ;
   cela nous évite de traiter votre après-midi comme un incident.
5. **Arrêtez-vous à la frontière** et dites-le. Si vous pensez qu'il y a
   quelque chose derrière une limite du périmètre, écrivez-le dans le rapport
   au lieu d'aller voir.

### Ce que nous nous engageons à faire

- **Accusé de réception immédiat**, automatique, à la soumission.
- **Tri sous 7 jours** par une personne, avec une raison écrite dans tous les
  cas — y compris le refus.
- **Pas de poursuite** contre quiconque respecte cette page de bonne foi. Si
  vous dépassez le périmètre par accident et nous le dites, cela reste couvert.
  C'est le sens de l'engagement : il vaut aussi quand vous vous êtes trompé.
- **Crédit public** au tableau d'honneur, sauf demande d'anonymat, qui est une
  case à cocher au moment du rapport.
- **Pas de récompense monétaire.** Cette plateforme n'a pas de revenus. Ce
  qu'elle donne est une attestation vérifiable, des fragments, et son
  tableau d'honneur. Le dire clairement vaut mieux que de laisser espérer.

### Comment signaler

`POST /api/security/reports`, depuis un compte. Le rapport demande une
reproduction, un impact et de préférence un vecteur CVSS. Les captures
s'envoient d'abord à `POST /api/security/reports/uploads`, qui rend une clé à
mettre dans `proof_keys` — ce ne sont pas des URL publiques, parce que la
preuve d'une faille non corrigée n'est pas un document public.

Si vous ne voulez pas de compte : `security@skill-uv.com`.

---

## What you may attack

### In scope

| Host | What it is | Note |
|---|---|---|
| `staging.skill-uv.com` | Staging | **The preferred target.** Fake data, reset nightly. |
| `api.skill-uv.com` | Production API | Your own test account only. Never anybody else's data. |
| `skill-uv.com` | Public front end | |
| `admin.skill-uv.com` | Admin panel | The login page is in scope; an admin session obtained is a critical finding to report at once. |
| `ctf.skill-uv.com` | Hosted training range | Deliberately vulnerable. Nothing found there is a finding. |

The source of the four public repositories is also in scope for reading:
`skilluv-backend`, `skilluv-frontend`, `skilluv-admin`, `skilluv-ia`. Reading
the code needs no permission. Testing the deployed service is what this page
is about.

### Out of scope

- `docs.skill-uv.com` — static, third-party hosted.
- The Discord server and anything reached through it.
- Third-party accounts and infrastructure: Brevo, Stripe, GitHub, Cloudflare,
  Coolify, Hetzner. Report to them directly.
- Any host not in the table above, even one that is obviously ours. A forgotten
  subdomain is a finding to report, not an invitation.

### Vulnerabilities we want

Injection (SQL, NoSQL, command, template), cross-site scripting in all three
forms, CSRF, IDOR and authorisation bypass, authentication bypass, SSRF, XXE,
path traversal, deserialisation, remote code execution, race conditions,
business logic flaws, cryptographic weaknesses, information disclosure through
the API or the logs, rate-limit bypass (documented, not exploited).

### Vulnerabilities out of scope

- **Denial of service of any kind.** Including load testing, including "just to
  see how much it takes". This is the one prohibition whose breach ends the
  relationship immediately.
- Brute force beyond the published limits — see research mode below.
- Social engineering, phishing, calling users or staff.
- Physical attacks.
- Missing security headers with no demonstrated impact. Send them together as
  hardening: useful, and not vulnerabilities.
- A dependency advisory with no reachability shown here. An entry in a
  dependency tree is not a finding.
- Raw scanner output. A report containing nothing a tool did not produce will
  be refused.
- Anything already on the hall of fame.

### Rules of engagement

1. **Take no more than you need** to prove it. One row proves an unauthorised
   read; a thousand rows are a leak you caused.
2. **Do not persist.** No backdoor, no admin account created, nothing left
   behind.
3. **Do not disclose** before coordination. The default embargo is 90 days from
   confirmation, and you are credited at publication.
4. **Identify yourself.** Add `X-Security-Research: <your Skilluv handle>` to
   your requests. It changes none of your rights; it stops us treating your
   afternoon as an incident.
5. **Stop at the boundary** and say so. If you believe there is something
   behind a limit of the scope, write that in the report instead of going to
   look.

### What we commit to

- **Immediate automated acknowledgement** on submission.
- **Triage within 7 days** by a person, with a written reason in every case —
  including a refusal.
- **No legal action** against anybody following this page in good faith. If you
  cross the boundary by accident and tell us, that stays covered. That is what
  the commitment is for: it holds when you got it wrong.
- **Public credit** on the hall of fame, unless you ask for anonymity, which is
  a checkbox on the report.
- **No money.** This platform has no revenue. What it gives is a verifiable
  attestation, fragments, and its hall of fame. Saying so plainly beats
  letting anybody hope.

### Research mode

The rate limiter allows what a person signing up needs, which is not what
testing a hundred payloads needs. So:

```
POST /api/security/research-token
```

returns a token. Send it as `X-Security-Research-Token` and your rate ceiling is
multiplied by ten. It grants nothing else — no capability, no data, no route —
and it does **not** remove the limit, because denial of service is out of scope
in fact and not only in this document.

Over five hundred requests a minute under one token revokes it automatically.
Re-issuing is one request. See `docs/security/RESEARCH-MODE.md`.

### How to report

`POST /api/security/reports`, from an account. The report asks for a
reproduction, an impact and preferably a CVSS vector. Screenshots go first to
`POST /api/security/reports/uploads`, which returns a key to put in
`proof_keys` — these are not public URLs, because proof of an unfixed
vulnerability is not a public document.

If you would rather not have an account: `security@skill-uv.com`.

### What happens next

`submitted` → `triaged` → `confirmed` → `fixed` → `published`, with a
notification at every step and a reason attached to every refusal. A reviewer
may open a **round** asking for a clearer reproduction, a proposed patch, or
arguing the severity; you answer on the report and the exchange is on the
record. Five rounds is the limit, after which somebody decides.

Full detail: `docs/security/DISCLOSURE-POLICY.md`.
