-- Ten pieces of audio the platform needs for itself.
--
-- Ticket T-03. Every other terrain in this domain is somebody else's — an
-- engine, a game, a sample library — and reaching a first credit through one
-- of them takes weeks of waiting on a maintainer. This is the terrain where
-- the platform is the client, the brief is real, and the work is heard by
-- everybody who opens the site.
--
-- ## Why the credit matters more here than the challenge
--
-- The seeded briefs below are ordinary. What makes this terrain worth having
-- is the second half of the ticket: the credit is displayed, in clear, on the
-- page the work ships on. A platform that asks its community for a soundtrack
-- and lists no names is asking for free work, and would deserve the reading.
--
-- `attestations.evidence_url` (0420) already carries where a credit appears,
-- and `audio_project_credited` already requires it. What was missing is the
-- other direction: a page cannot ask "who is credited here". The view below
-- answers that, so the landing page renders from the same record the profile
-- does rather than from a list somebody maintains by hand.
--
-- ## Why they are drafts, again
--
-- Same reason as 0417: a brief nobody has reviewed must not be offered to
-- somebody learning. These need whoever owns each canvas game to say what the
-- thing should sound like.

INSERT INTO challenge_templates
    (title, description, instructions, skill_domain, difficulty, language,
     status, is_training, ai_policy, evaluation_rubric)
SELECT
    c.title,
    c.description,
    '## Ce qu''il y a à faire' || E'\n\n' ||
    c.description || E'.\n\n' ||
    '## Ce qui est attendu' || E'\n\n' ||
    c.expected || E'\n\n' ||
    'Ce travail est destiné à être publié sur skill-uv.com, avec ton nom en ' ||
    'clair sur la page concernée. Le crédit est enregistré comme attestation, ' ||
    'et il pointe vers la page où il apparaît.' || E'\n\n' ||
    'Dans tous les cas : chaque source utilisée est déclarée avec sa licence, ' ||
    'ou tout est original et c''est écrit. Le niveau est mesuré (LUFS, crête ' ||
    'vraie) et adapté à la destination. Un travail sans documentation est refusé.' || E'\n\n' ||
    '## Ce qui sera regardé' || E'\n\n' ||
    'La grille de revue de la famille s''applique, et elle est publique.',
    'audio', c.difficulty, NULL,
    'draft', TRUE, c.ai_policy,
    COALESCE(
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'audio' AND g.reviewer_group = o.reviewer_group),
        (SELECT g.criteria FROM review_grids g
          WHERE g.domain = 'audio' AND g.reviewer_group IS NULL)
    )
FROM (VALUES

('audio-composer', 'Skilluv Coder Battle — thème principal',
 'Écrire le thème du jeu Coder Battle : tendu sans être agressif, tenable en boucle pendant une partie de dix minutes',
 'Le master, la boucle vérifiée sans couture, les stems.', 3, 'disclosure_required'),

('audio-composer', 'Skilluv Coder Battle — thème de fin de manche',
 'Quinze secondes qui disent qui a gagné sans le dire, jouées à la fin de chaque manche',
 'Deux versions — victoire et défaite — du même matériau.', 2, 'disclosure_required'),

('audio-composer', 'Skilluv Craft Journey — bande originale en 5 morceaux',
 'Cinq morceaux pour les cinq étapes du parcours : arrivée, apprentissage, première contribution, revue, compagnonnage',
 'Cinq masters partageant une identité, leurs stems, et la note qui montre le matériau commun.', 5, 'disclosure_required'),

('audio-composer', 'Skilluv — générique de la chaîne communautaire',
 'Une entrée de dix secondes et une sortie de quinze pour les vidéos et les lives de la communauté',
 'Les deux pièces, plus une version allégée pour passer sous la parole.', 2, 'disclosure_required'),

('audio-sound-designer', 'Skilluv — pack d''interface de la plateforme',
 'Douze sons pour l''interface du site : progression, réussite, erreur, notification, ouverture, validation',
 'Les douze sons, discrets et supportables à la centième écoute, avec leur feuille d''usage.', 3, 'disclosure_required'),

('audio-sound-designer', 'Skilluv — sons de progression et de rang',
 'Les sons des moments qui comptent : compétence acquise, défi validé, rang franchi, attestation délivrée',
 'Quatre sons d''une même famille, gradués en intensité selon l''importance du moment.', 3, 'disclosure_required'),

('audio-sound-designer', 'Skilluv Coder Battle — pack de jeu',
 'Quinze sons de jeu : frappe, compilation, test qui passe, test qui casse, minuteur, fin de manche',
 'Les quinze sons, la démonstration en contexte, la feuille d''usage.', 3, 'disclosure_required'),

('audio-voice-actor', 'Skilluv — voix du narrateur des jeux canvas',
 'La voix qui accompagne les jeux de la plateforme : accueil, consignes, encouragement, fin de partie',
 'Les prises montées, plus les variantes pour les répliques répétées souvent.', 3, 'human_verified'),

('audio-voice-actor', 'Skilluv — voix d''accueil des nouveaux arrivants',
 'Trente secondes lues à qui arrive sur la plateforme pour la première fois. Chaleureux sans être commercial',
 'La prise montée, une alternative de ton, et l''étendue d''usage écrite.', 2, 'human_verified'),

('audio-music-implementer', 'Skilluv Coder Battle — intégration musicale adaptative',
 'Faire réagir la musique du jeu au temps restant et à l''écart de score, sans casser la mesure',
 'Le projet middleware ou le code d''intégration, et une build jouable où l''on peut forcer chaque état.', 4, 'disclosure_required')

) AS c(orientation_slug, title, description, expected, difficulty, ai_policy)
JOIN orientations o ON o.slug = c.orientation_slug;

-- ═══════════════════════════════════════════════════════════════════
-- Who is credited on what
-- ═══════════════════════════════════════════════════════════════════
--
-- A view rather than a table: every fact in it already exists, and a second
-- copy would be one somebody has to remember to update when an attestation is
-- revoked. Revoked ones are excluded here, which is the whole point — a credit
-- the platform has retracted must leave the page it was printed on.

CREATE VIEW work_credits AS
SELECT p.id            AS project_id,
       p.slug          AS project_slug,
       u.id            AS user_id,
       u.username,
       u.display_name,
       a.basis,
       a.title         AS credit_title,
       a.evidence_url,
       a.verification_code,
       a.issued_at,
       ps.audio_subtype,
       ps.primary_domain
  FROM attestations a
  JOIN users u ON u.id = a.user_id
  JOIN deliverables d ON d.id = ANY (a.linked_deliverable_ids)
  JOIN project_slices ps ON ps.id = d.slice_id
  JOIN projects p ON p.id = ps.project_id
 WHERE a.basis = 'audio_project_credited'
   AND a.revoked_at IS NULL
   AND d.revoked_at IS NULL;

COMMENT ON VIEW work_credits IS
    'Who is credited on which project, from the attestations that carry it. A '
    'view rather than a table: a second copy of these facts is one somebody '
    'has to remember to update when a credit is revoked, and a retracted '
    'credit has to leave the page it was printed on.';
