-- Where audio work can actually be done, proposed rather than declared.
--
-- ## Why these are not rows in `projects`
--
-- The backlog says "seed the OSS projects that welcome audio contributions",
-- and the obvious reading is fourteen rows in `projects`. That table refuses
-- them, and it is right to: `owner_type` is `user` or `guild` and `owner_id`
-- is NOT NULL, because a terrain on this platform is something a person
-- answers for. A steward greets newcomers, decides which slices are worth
-- opening, and takes the blame when somebody's first contribution is wasted.
--
-- A migration cannot appoint that person. Inserting the projects with a
-- fabricated owner would put fourteen terrains on the platform that look
-- staffed and are not — which is the specific failure the steward role exists
-- to prevent, and the worst possible first experience for somebody arriving
-- in a new domain.
--
-- So the seed is what it honestly is: a shortlist somebody researched, with
-- the reason it is on the list and the labels to watch, waiting for a steward
-- to adopt it. `adopted_project_id` is filled when one does, and that is the
-- moment the terrain becomes real.
--
-- ## Why the table is not audio's
--
-- Every domain arrives the same way — a list of upstream projects somebody
-- believes in, and no steward yet. Code and AI did it in a spreadsheet.

CREATE TABLE terrain_proposals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(80) NOT NULL UNIQUE,
    name VARCHAR(120) NOT NULL,
    skill_domain VARCHAR(30) NOT NULL REFERENCES skill_domains(slug) ON UPDATE CASCADE,

    kind VARCHAR(30) NOT NULL CHECK (kind IN (
        -- An upstream repository that takes contributions.
        'oss_repo',
        -- A community library where work is published rather than merged:
        -- Freesound, OpenGameArt. The contribution is the upload.
        'community_library',
        -- Something the platform itself needs built.
        'internal'
    )),
    upstream_url TEXT NOT NULL CHECK (upstream_url ~ '^https://'),
    -- The labels to watch when ingesting slices from this project. Empty for
    -- the ones with no issue tracker to watch.
    ingestion_labels TEXT[] NOT NULL DEFAULT '{}',
    -- Why this is a good place to start, in the words somebody arriving reads.
    why_md TEXT NOT NULL CHECK (btrim(why_md) <> ''),

    -- Filled when a steward takes it on. Until then the proposal is a
    -- shortlist entry and nothing more.
    adopted_project_id UUID REFERENCES projects(id) ON DELETE SET NULL,
    adopted_at TIMESTAMPTZ,
    -- Set when somebody looked and decided against, with the reason. A list
    -- that only grows tells nobody what was already considered.
    declined_at TIMESTAMPTZ,
    declined_reason TEXT,

    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT terrain_proposal_adoption_is_complete CHECK (
        (adopted_project_id IS NULL) = (adopted_at IS NULL)
    ),
    CONSTRAINT terrain_proposal_decline_says_why CHECK (
        declined_at IS NULL OR btrim(COALESCE(declined_reason, '')) <> ''
    ),
    CONSTRAINT terrain_proposal_is_not_both CHECK (
        adopted_at IS NULL OR declined_at IS NULL
    )
);

COMMENT ON TABLE terrain_proposals IS
    'Shortlisted places to work, per domain, waiting for a steward. Not rows '
    'in `projects`: a terrain there has an owner who answers for it, and a '
    'migration cannot appoint one. A terrain that looks staffed and is not is '
    'the worst first experience the platform can offer.';

CREATE INDEX idx_terrain_proposals_open
    ON terrain_proposals (skill_domain, sort_order)
    WHERE adopted_at IS NULL AND declined_at IS NULL;

INSERT INTO terrain_proposals
    (slug, name, skill_domain, kind, upstream_url, ingestion_labels, why_md, sort_order) VALUES

-- ── Engines: the audio subsystem itself ────────────────────────────
('godot-audio', 'Godot Engine — sous-système audio', 'audio', 'oss_repo',
 'https://github.com/godotengine/godot',
 ARRAY['audio', 'topic:audio'],
 'Le moteur libre le plus utilisé, et son audio est un domaine où les tickets ouverts sont nombreux et lisibles. Une correction ici est utilisée par des milliers de jeux, et le processus de revue est public de bout en bout.', 10),

('bevy-audio', 'Bevy — écosystème audio', 'audio', 'oss_repo',
 'https://github.com/bevyengine/bevy',
 ARRAY['A-Audio'],
 'Un moteur en Rust dont la couche audio est jeune : c''est l''un des rares endroits où un programmeur audio débutant peut proposer une brique qui manque plutôt que corriger celle d''un autre.', 20),

-- ── Games that need music and sound ────────────────────────────────
('0-ad-audio', '0 A.D. — musique et bruitage', 'audio', 'oss_repo',
 'https://gitea.wildfiregames.com/0ad/0ad',
 ARRAY['audio', 'sound', 'music'],
 'Un jeu de stratégie libre en développement depuis vingt ans, avec un besoin permanent de musique et d''ambiances, et une communauté habituée à accueillir des contributeurs non programmeurs.', 30),

('wesnoth-audio', 'Battle for Wesnoth — musique et bruitage', 'audio', 'oss_repo',
 'https://github.com/wesnoth/wesnoth',
 ARRAY['Audio', 'Music'],
 'Une des rares communautés de jeu libre où les contributions musicales sont explicitement documentées et créditées. Le chemin entre une première pièce et son intégration est court.', 40),

('openttd-audio', 'OpenTTD — modernisation sonore', 'audio', 'oss_repo',
 'https://github.com/OpenTTD/OpenTTD',
 ARRAY['audio'],
 'Un jeu dont les sons d''origine datent de 1995 et dont la communauté refait progressivement les ressources. Un terrain idéal pour du bruitage à contrainte forte : remplacer sans trahir.', 50),

('endless-sky-audio', 'Endless Sky — ambiances spatiales', 'audio', 'oss_repo',
 'https://github.com/endless-sky/endless-sky',
 ARRAY['audio', 'sound'],
 'Un jeu spatial libre où les ambiances comptent plus que les effets, et où la barre d''entrée est basse pour une première contribution sonore.', 60),

-- ── Community libraries: publishing is the contribution ────────────
('freesound', 'Freesound', 'audio', 'community_library',
 'https://freesound.org',
 '{}',
 'La plus grande banque de sons sous licence libre. Publier un pack cohérent ici, avec ses licences propres, est un artefact opposable : n''importe qui peut l''écouter, le télécharger et voir combien de projets s''en servent.', 110),

('opengameart-audio', 'OpenGameArt — audio', 'audio', 'community_library',
 'https://opengameart.org',
 '{}',
 'Là où les développeurs de jeux libres cherchent leurs sons et leur musique. Une contribution ici a des chances réelles de se retrouver dans un jeu publié — et le crédit qui va avec.', 120),

('bandlab-collab', 'BandLab', 'audio', 'community_library',
 'https://www.bandlab.com',
 '{}',
 'Pour la collaboration entre musiciens plutôt que pour la publication : un terrain d''entraînement où l''on écrit à plusieurs, ce qu''aucun défi solitaire n''apprend.', 130),

-- ── Internal ───────────────────────────────────────────────────────
('skilluv-canvas-audio', 'Jeux Skilluv — bande-son et sons d''interface', 'audio', 'internal',
 'https://skill-uv.com',
 '{}',
 'Les jeux et les interfaces de la plateforme ont besoin de son, et le crédit est affiché en clair sur les pages concernées. Le terrain le plus court entre un premier travail et un usage réel.', 210);
