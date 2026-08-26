-- The confidentiality agreement a mission can require, as versioned text.
--
-- ## Why the text is a row and not a file
--
-- Because a signature has to name the bytes it agreed to. 0557 records the
-- SHA-256 of the document a person was shown, and that only means something if
-- the platform can still produce those exact bytes years later — after the
-- wording has been improved twice and a lawyer has been through it.
--
-- A markdown file in the repository cannot do that: it has one current version
-- and a git history nobody is going to diff during a dispute. A row with a
-- version and `is_current` can, and an old signature keeps pointing at the text
-- that was actually on the screen.
--
-- ## Why this is not `content_guides`
--
-- That table is keyed by `skill_domain NOT NULL` and its `kind` is a closed
-- list of onboarding, toolkit and template material. A confidentiality
-- agreement belongs to no domain — a design mission and a penetration test sign
-- the same one — and filing it under a domain would mean either picking one
-- arbitrarily or adding a nullable domain to a table whose whole shape is
-- per-domain content.
--
-- ## What this text is, and is not
--
-- It is a draft written by the people building this platform. No lawyer has
-- read it. `is_reviewed` says so on the row, the endpoint that serves it says so
-- in the response, and `docs/security/LEGAL.md` says so in the same words —
-- because the failure mode of a self-drafted agreement is not that it is bad,
-- it is that everybody assumes somebody checked.
--
-- Two templates and not three. M-06 asked for three levels; the first was "no
-- agreement", which is `missions.nda_required = FALSE` and does not need a
-- document.

CREATE TABLE mission_nda_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Matches `mission_nda_signatures.template`.
    slug VARCHAR(20) NOT NULL
        CHECK (slug IN ('mutual_standard', 'mutual_extended')),
    locale VARCHAR(5) NOT NULL CHECK (locale IN ('en', 'fr')),
    version SMALLINT NOT NULL CHECK (version >= 1),

    title VARCHAR(160) NOT NULL,
    -- The text itself. Served verbatim and hashed as served.
    body_md TEXT NOT NULL CHECK (length(btrim(body_md)) >= 200),

    -- Which version is offered now. Old ones stay, because a signature points
    -- at them.
    is_current BOOLEAN NOT NULL DEFAULT TRUE,
    -- Whether a lawyer has read this version. False, and said out loud rather
    -- than left to be assumed.
    is_reviewed BOOLEAN NOT NULL DEFAULT FALSE,
    reviewed_note TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (slug, locale, version)
);

COMMENT ON TABLE mission_nda_templates IS
    'Versioned confidentiality-agreement text. A row rather than a file because '
    'a signature records the hash of what was shown, and that only means '
    'something if those exact bytes can still be produced years later.';

COMMENT ON COLUMN mission_nda_templates.is_reviewed IS
    'Whether a lawyer has read this version. Stated on the row rather than '
    'assumed: the failure mode of a self-drafted agreement is that everybody '
    'believes somebody checked.';

-- One current version per template and language.
CREATE UNIQUE INDEX uniq_mission_nda_current
    ON mission_nda_templates (slug, locale) WHERE is_current;

-- ═══════════════════════════════════════════════════════════════════
-- What a mission says about its agreement
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE missions
    ADD COLUMN nda_template VARCHAR(20)
        CHECK (nda_template IS NULL OR nda_template IN (
            'mutual_standard', 'mutual_extended', 'client_custom'
        )),
    -- Where the client's own agreement lives, when they brought one.
    ADD COLUMN nda_document_url VARCHAR(500)
        CHECK (nda_document_url IS NULL OR nda_document_url ~ '^(https://|/)');

ALTER TABLE missions
    -- A mission that requires an agreement says which one. Without this the
    -- flag was advice: `nda_required` has existed since 0192 and nothing has
    -- ever been able to act on it.
    ADD CONSTRAINT an_nda_requirement_names_its_document CHECK (
        NOT nda_required OR nda_template IS NOT NULL
    ),
    -- And a client's own agreement says where it is.
    ADD CONSTRAINT a_custom_nda_says_where CHECK (
        nda_template <> 'client_custom' OR nda_document_url IS NOT NULL
    );

COMMENT ON COLUMN missions.nda_template IS
    'Which agreement applies. Required whenever nda_required is true, which is '
    'what turns that flag from advice into something the application flow can '
    'refuse on.';

-- ═══════════════════════════════════════════════════════════════════
-- The two drafts
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO mission_nda_templates (slug, locale, version, title, body_md, is_reviewed)
VALUES

('mutual_standard', 'en', 1, 'Mutual confidentiality agreement — standard',
$md$# Mutual confidentiality agreement

**Draft. No lawyer has reviewed this text.** It is offered so that a mission
can be published with something written down rather than nothing, and it will
be replaced by a reviewed version. Do not rely on it for an engagement whose
disclosure would seriously harm either party.

## 1. Parties

This agreement is between the organisation that published the mission (the
**client**) and the person accepted to work on it (the **contributor**). Skilluv
provides the platform and is not a party to it.

## 2. What is confidential

Anything either party discloses to the other in connection with the mission and
which is not already public, including: the scope, systems, code,
configurations, credentials, personal data, business information, and — for a
security engagement specifically — every finding, its details and its existence.

## 3. What is not confidential

Information that: was already public; was already known to the receiving party
without an obligation of confidence; is independently developed without use of
the disclosed information; or becomes public other than through a breach of this
agreement.

Methodologies, tooling and general skill the contributor brings or develops
remain theirs and are not confidential information, provided they carry nothing
specific to the client.

## 4. Obligations

Each party will: use the other's confidential information only for the mission;
disclose it to nobody else without written permission; protect it with at least
the care it uses for its own confidential information; and, for a security
engagement, take no more data than is needed to demonstrate a finding.

## 5. Duration

Two years from the date of signature. Personal data and anything protected by
law for longer remains protected for as long as that law requires.

## 6. Disclosure of findings

Nothing in this agreement prevents the contributor from describing the
engagement in general terms — the type of work, its duration, the number and
severity of findings — unless the mission states otherwise. Whether the
contributor may publish details, and whether they are credited, is set by the
mission's own disclosure terms and not by this agreement.

## 7. Legal disclosure

Either party may disclose confidential information where a law, a court or a
regulator requires it, having first told the other party where it is lawful to
do so.

## 8. Return

On request at the end of the mission, each party will delete or return the
other's confidential information, other than one copy kept for the purpose of
demonstrating what was agreed and delivered.

## 9. No licence

Nothing here transfers ownership of anything. What the client owns and what the
contributor owns is set by the mission's intellectual-property terms.

## 10. Signature

Accepted electronically on this platform. The record of that acceptance
includes the date, the address it came from, and the cryptographic hash of this
exact text — so that both parties can establish later what was actually agreed.
$md$, FALSE),

('mutual_standard', 'fr', 1, 'Accord de confidentialité mutuel — standard',
$md$# Accord de confidentialité mutuel

**Projet. Aucun juriste n'a relu ce texte.** Il existe pour qu'une mission
puisse être publiée avec quelque chose d'écrit plutôt que rien, et il sera
remplacé par une version relue. Ne pas s'y fier pour une mission dont la
divulgation nuirait sérieusement à l'une des parties.

## 1. Parties

Cet accord lie l'organisation qui a publié la mission (le **client**) et la
personne retenue pour la réaliser (le **contributeur**). Skilluv fournit la
plateforme et n'est pas partie à l'accord.

## 2. Ce qui est confidentiel

Tout ce qu'une partie communique à l'autre à propos de la mission et qui n'est
pas déjà public : le périmètre, les systèmes, le code, les configurations, les
identifiants, les données personnelles, les informations d'affaires — et, pour
une mission de sécurité, chaque vulnérabilité, ses détails et son existence
même.

## 3. Ce qui ne l'est pas

Une information qui : était déjà publique ; était déjà connue de la partie qui
la reçoit sans obligation de confidentialité ; est développée de façon
indépendante sans utiliser l'information communiquée ; ou devient publique
autrement que par un manquement au présent accord.

Les méthodes, les outils et le savoir-faire général que le contributeur apporte
ou développe restent les siens et ne sont pas des informations
confidentielles, à condition qu'ils ne contiennent rien de spécifique au
client.

## 4. Obligations

Chaque partie s'engage à : n'utiliser les informations confidentielles de
l'autre que pour la mission ; ne les communiquer à personne sans accord écrit ;
les protéger avec au moins le soin qu'elle applique aux siennes ; et, pour une
mission de sécurité, ne pas extraire plus de données que nécessaire pour
démontrer une vulnérabilité.

## 5. Durée

Deux ans à compter de la signature. Les données personnelles et tout ce qu'une
loi protège plus longtemps restent protégés aussi longtemps que cette loi
l'exige.

## 6. Divulgation des découvertes

Rien ici n'interdit au contributeur de décrire la mission en termes généraux —
le type de travail, sa durée, le nombre et la gravité des découvertes — sauf
mention contraire de la mission. La possibilité de publier des détails, et le
fait d'être crédité, relèvent des conditions de divulgation de la mission et
non du présent accord.

## 7. Divulgation légale

Chaque partie peut communiquer une information confidentielle lorsqu'une loi,
un tribunal ou une autorité l'exige, après en avoir informé l'autre partie
lorsque la loi le permet.

## 8. Restitution

Sur demande à la fin de la mission, chaque partie supprime ou restitue les
informations confidentielles de l'autre, à l'exception d'un exemplaire conservé
pour pouvoir établir ce qui a été convenu et livré.

## 9. Absence de licence

Rien ici ne transfère la propriété de quoi que ce soit. Ce qui appartient au
client et ce qui appartient au contributeur est défini par les conditions de
propriété intellectuelle de la mission.

## 10. Signature

Acceptation électronique sur cette plateforme. L'enregistrement de cette
acceptation comprend la date, l'adresse d'origine et l'empreinte
cryptographique de ce texte exact — afin que les deux parties puissent établir
plus tard ce qui a réellement été convenu.
$md$, FALSE),

('mutual_extended', 'en', 1, 'Mutual confidentiality agreement — extended',
$md$# Mutual confidentiality agreement (extended)

**Draft. No lawyer has reviewed this text.** See the standard agreement's
warning; it applies here with more force, because this version restricts what
the contributor may do afterwards.

This agreement contains everything in the standard mutual agreement, with the
following changes.

## 5. Duration (replaces clause 5)

Five years from the date of signature.

## 6. Disclosure of findings (replaces clause 6)

The contributor will not describe this engagement publicly, in any terms, for
the duration of this agreement, other than: the fact that they carried out a
mission of this type through this platform, its duration, and the number of
findings by severity — with no detail identifying the client or its systems.

## 11. Non-solicitation (additional)

For six months after the mission ends, neither party will engage the other
outside this platform for work of the same kind that was introduced by it.

This clause exists because the platform's costs are paid by the commission on
missions it arranged, and an introduction that moves off-platform immediately is
a cost with no revenue behind it. It restricts the *engagement*, not the
people: nothing here prevents either party from working with anybody, on
anything else, at any time.

## 12. Personnel (additional)

The contributor will not name the client as a reference, in a portfolio or
elsewhere, without written permission.

## 13. Consequences (additional)

A breach of this agreement may end the mission immediately, may forfeit unpaid
amounts for work not yet accepted, and may be pursued under the law that applies
to it. Skilluv is not the judge of that: the platform records what was agreed
and what happened, and the parties or a court decide the rest.
$md$, FALSE),

('mutual_extended', 'fr', 1, 'Accord de confidentialité mutuel — renforcé',
$md$# Accord de confidentialité mutuel (renforcé)

**Projet. Aucun juriste n'a relu ce texte.** L'avertissement de l'accord
standard s'applique ici avec plus de force encore, car cette version restreint
ce que le contributeur peut faire ensuite.

Cet accord reprend l'intégralité de l'accord mutuel standard, avec les
modifications suivantes.

## 5. Durée (remplace l'article 5)

Cinq ans à compter de la signature.

## 6. Divulgation des découvertes (remplace l'article 6)

Le contributeur ne décrira pas publiquement cette mission, en quelque terme que
ce soit, pendant la durée du présent accord, à l'exception : du fait qu'il a
réalisé une mission de ce type via cette plateforme, de sa durée, et du nombre
de découvertes par niveau de gravité — sans aucun détail identifiant le client
ou ses systèmes.

## 11. Non-sollicitation (ajout)

Pendant six mois après la fin de la mission, aucune des deux parties
n'engagera l'autre en dehors de cette plateforme pour un travail de même nature
que celui qu'elle a permis.

Cet article existe parce que les frais de la plateforme sont couverts par la
commission sur les missions qu'elle a organisées, et qu'une mise en relation qui
part immédiatement ailleurs est un coût sans recette. Il restreint
l'*engagement*, pas les personnes : rien ici n'empêche l'une ou l'autre partie
de travailler avec qui elle veut, sur autre chose, à tout moment.

## 12. Références (ajout)

Le contributeur ne citera pas le client comme référence, dans un portfolio ou
ailleurs, sans accord écrit.

## 13. Conséquences (ajout)

Un manquement au présent accord peut mettre fin immédiatement à la mission,
peut faire perdre les sommes non versées pour un travail non encore accepté, et
peut être poursuivi selon la loi applicable. Skilluv n'en est pas juge : la
plateforme enregistre ce qui a été convenu et ce qui s'est passé, les parties ou
un tribunal décident du reste.
$md$, FALSE);
