-- The audio craft score: a formula in rows, and the words that go next to it.
--
-- ## Nothing new is created here
--
-- Migration 0195 built `craft_score_weights`, 0204 gave every domain its
-- tiers — audio included, three hundred migrations before audio existed — and
-- 0300 established that a domain contributes weights and a measuring service,
-- never a new table. This is the third set of weights.
--
-- ## The tiers are the shared ones, and the backlog's are not used
--
-- The backlog proposed `apprentice / hobbyist / semi-pro / pro / veteran` with
-- thresholds of its own. 0300 refused the equivalent for AI and the reasoning
-- is unchanged: a tier is a position on a scale, each scale is calibrated by
-- its own weights, and a domain with private vocabulary means nobody can
-- compare a profile to itself across two domains. Somebody who is `senior` in
-- code and `senior` in audio has travelled a comparable distance; `pro` next
-- to `staff` would say nothing.
--
-- ## The ratios, and what they say
--
-- Three deserve their reasoning out loud.
--
--   * **a credit is worth twice a composition.** A published piece proves the
--     craft; a credit on something that shipped proves somebody trusted it
--     enough to build on, and that the work survived contact with a
--     production. In this domain that is the rarer of the two by a wide
--     margin, and it is what a client is actually buying.
--   * **an adaptive system outweighs everything except a mission.** It is the
--     only artefact here that cannot be produced alone: it requires a game to
--     integrate into, an engineer to agree, and a build to prove it in.
--   * **reach is logarithmic and modest.** Plays depend on the genre and on
--     who shared it as much as on the craft. One track going around the world
--     should be visible in the score and must not outweigh a career.
--
-- ## What is deliberately not counted
--
-- Followers, and the count of portfolio items on their own. The backlog gave
-- three points per imported track. An import is a URL somebody typed: paying
-- for it by the item pays for typing. `portfolio_reach` counts plays, which
-- at least happened, and it is capped by being logarithmic.

INSERT INTO craft_score_weights
    (skill_domain, term, weight, kind, baseline, explanation, sort_order)
VALUES
    ('audio', 'attestations_audio', 5, 'count', NULL,
     'Chaque attestation audio délivrée.', 10),
    ('audio', 'compositions_published', 40, 'count', NULL,
     'Chaque composition originale livrée, écoutable, licences en règle.', 20),
    ('audio', 'soundpacks_delivered', 30, 'count', NULL,
     'Chaque pack sonore livré — cohérent, nommé, utilisable tel quel.', 30),
    ('audio', 'voice_reels_validated', 25, 'count', NULL,
     'Chaque bande démo jugée exploitable par un relecteur du métier.', 40),
    ('audio', 'adaptive_systems_shipped', 100, 'count', NULL,
     'Chaque système musical adaptatif intégré et vérifié dans une build. '
     'Le seul artefact du domaine qu''on ne peut pas produire seul : il faut '
     'un jeu où l''intégrer et quelqu''un qui accepte.', 50),
    ('audio', 'programming_contributions', 60, 'count', NULL,
     'Chaque fonctionnalité audio livrée dans un moteur ou une bibliothèque.', 60),
    ('audio', 'projects_credited', 80, 'count', NULL,
     'Chaque œuvre publiée portant un crédit. Vaut le double d''une composition : '
     'le travail a survécu à une production, et c''est ce qu''un client achète.', 70),
    ('audio', 'missions_completed', 100, 'count', NULL,
     'Chaque mission audio rémunérée menée à son terme.', 80),
    ('audio', 'portfolio_reach', 30, 'log_scaled', NULL,
     'Les écoutes cumulées sur les plateformes déclarées, sur une échelle '
     'logarithmique : un million vaut environ le double de mille. L''audience '
     'dépend du genre autant que du métier.', 90),
    ('audio', 'review_grid_average', 200, 'offset_scaled', 3.0,
     'La moyenne des grilles de relecture, comptée à partir de 3 sur 5 : le '
     'milieu de la grille ne vaut rien, ce qui compte est l''écart.', 100),
    ('audio', 'orientations_distinct', 20, 'count', NULL,
     'Chaque métier audio dans lequel un artefact vérifié existe.', 110),
    ('audio', 'years_active', 25, 'count', NULL,
     'Chaque année depuis le premier artefact audio vérifié.', 120),
    ('audio', 'featured_times', 200, 'count', NULL,
     'Chaque mise en avant éditoriale.', 130);
