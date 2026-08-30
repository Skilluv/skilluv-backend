-- Which trade each design challenge belongs to.
--
-- ## The gap
--
-- Migration 0239 seeds 130 design challenges, five per trade across the 26
-- orientations. It knows the orientation while it inserts — it uses it to pick
-- the review grid — and then the link is gone. `challenge_templates` has a
-- `skill_domain`, which says "design", and nothing that says which of the 26
-- design trades.
--
-- So the catalogue cannot answer the one question anybody publishing it has:
-- *show me the five drafts for design-motion*. That, and not the writing, is
-- what has blocked SKI-349 — an authoring surface cannot list what the schema
-- cannot group.
--
-- ## The backfill
--
-- The 130 pairs below are the (orientation, title) list of migration 0239,
-- read back out of it rather than retyped. No title appears twice, verified
-- before writing this, so matching on the title is exact.
--
-- Matching on the title rather than on the id because 0239 inserts with
-- `ON CONFLICT DO NOTHING` and returns nothing: there is no id to have kept.
--
-- ## Why nullable, and why no default
--
-- Most challenges in this table belong to a domain and to no particular trade
-- — that is true of every other domain's catalogue and is not a gap. The
-- column says "this one is a trade's", and NULL says "this one is the
-- domain's".

ALTER TABLE challenge_templates
    ADD COLUMN orientation_id UUID REFERENCES orientations(id) ON DELETE SET NULL;

COMMENT ON COLUMN challenge_templates.orientation_id IS
    'The trade this challenge belongs to, when it belongs to one. NULL for a '
    'challenge that is the domain''s rather than one trade''s. Set for the 130 '
    'design seeds of migration 0239, which knew their orientation and had '
    'nowhere to record it.';

CREATE INDEX idx_challenge_templates_orientation
    ON challenge_templates (orientation_id, status)
    WHERE orientation_id IS NOT NULL;

-- ═══════════════════════════════════════════════════════════════════
-- The 130, put back where they came from
-- ═══════════════════════════════════════════════════════════════════

UPDATE challenge_templates ct
   SET orientation_id = o.id
  FROM (VALUES
    ('design-product', 'Refonte d''un parcours d''inscription'),
    ('design-product', 'Les quatre états d''un écran'),
    ('design-product', 'Un test d''utilisabilité sur cinq personnes'),
    ('design-product', 'Une fonctionnalité de bout en bout'),
    ('design-product', 'Reprendre un écran sur retours utilisateurs'),
    ('design-system', 'Une échelle de couleurs sémantique'),
    ('design-system', 'Un composant et toutes ses variantes'),
    ('design-system', 'Une échelle typographique'),
    ('design-system', 'Un audit d''incohérences'),
    ('design-system', 'Un modèle de contribution'),
    ('design-ai-conversational', 'Un flux de conversation et ses réparations'),
    ('design-ai-conversational', 'La persona d''un assistant'),
    ('design-ai-conversational', 'Afficher l''incertitude'),
    ('design-ai-conversational', 'Consentement et transparence'),
    ('design-ai-conversational', 'Une interface vocale courte'),
    ('design-web', 'Une page d''atterrissage qui convertit'),
    ('design-web', 'Une fiche produit e-commerce'),
    ('design-web', 'Un tunnel de commande'),
    ('design-web', 'Une refonte sous contrainte CMS'),
    ('design-web', 'Un budget de performance tenu'),
    ('design-editorial-web', 'Un article long lisible'),
    ('design-editorial-web', 'Une grille éditoriale'),
    ('design-editorial-web', 'Un récit au défilement'),
    ('design-editorial-web', 'Une page d''accueil de magazine'),
    ('design-editorial-web', 'Intégrer l''illustration au texte'),
    ('design-mobile', 'Un écran natif sur les deux plateformes'),
    ('design-mobile', 'Une navigation à une main'),
    ('design-mobile', 'Un écran qui survit au réseau'),
    ('design-mobile', 'Un parcours de permissions'),
    ('design-mobile', 'Une fiche de mise en avant'),
    ('design-motion-ui', 'Une transition entre deux écrans'),
    ('design-motion-ui', 'Un état de chargement qui rassure'),
    ('design-motion-ui', 'Un logo animé'),
    ('design-motion-ui', 'Un système de micro-interactions'),
    ('design-motion-ui', 'Respecter le mouvement réduit'),
    ('design-motion-2d', 'Une séquence narrative de trente secondes'),
    ('design-motion-2d', 'De la typographie cinétique'),
    ('design-motion-2d', 'Une boucle sans raccord'),
    ('design-motion-2d', 'Une explication animée'),
    ('design-motion-2d', 'Une déclinaison multi-format'),
    ('design-motion-3d', 'Une animation produit'),
    ('design-motion-3d', 'Un habillage en 3D'),
    ('design-motion-3d', 'Une simulation maîtrisée'),
    ('design-motion-3d', 'Une chaîne de rendu documentée'),
    ('design-motion-3d', 'Un plan photoréaliste et sa version stylisée'),
    ('design-video', 'Un montage narratif court'),
    ('design-video', 'Un étalonnage cohérent'),
    ('design-video', 'Un mixage propre'),
    ('design-video', 'Un sous-titrage utilisable'),
    ('design-video', 'Une déclinaison verticale'),
    ('design-brand-identity', 'Un logotype qui tient aux deux extrêmes'),
    ('design-brand-identity', 'Une identité complète pour une structure locale'),
    ('design-brand-identity', 'Un système de déclinaison'),
    ('design-brand-identity', 'Une refonte qui garde la reconnaissance'),
    ('design-brand-identity', 'Une identité pour un support ingrat'),
    ('design-typography', 'Un alphabet de base'),
    ('design-typography', 'Approche et crénage'),
    ('design-typography', 'Un axe variable'),
    ('design-typography', 'Une couverture multi-écriture'),
    ('design-typography', 'Des fonctionnalités OpenType'),
    ('design-naming-verbal', 'Un nom et sa défense'),
    ('design-naming-verbal', 'Un ton de voix documenté'),
    ('design-naming-verbal', 'Un récit de marque'),
    ('design-naming-verbal', 'Une accroche et ses variantes'),
    ('design-naming-verbal', 'Un lexique produit'),
    ('design-illustration', 'Une illustration éditoriale'),
    ('design-illustration', 'Une série cohérente'),
    ('design-illustration', 'Des états vides illustrés'),
    ('design-illustration', 'Une image explicative'),
    ('design-illustration', 'Des valeurs qui tiennent en gris'),
    ('design-iconography', 'Un jeu de douze icônes'),
    ('design-iconography', 'Des métaphores lisibles ailleurs'),
    ('design-iconography', 'Un jeu à trois tailles'),
    ('design-iconography', 'Une livraison prête à intégrer'),
    ('design-iconography', 'Un audit d''équilibre optique'),
    ('design-character', 'Une silhouette reconnaissable'),
    ('design-character', 'Une planche d''expressions'),
    ('design-character', 'Une tournette complète'),
    ('design-character', 'Une mascotte de marque'),
    ('design-character', 'Un personnage prêt à rigger'),
    ('design-dataviz', 'Le bon graphique pour la question'),
    ('design-dataviz', 'Un tableau de bord de six indicateurs'),
    ('design-dataviz', 'Une infographie narrative'),
    ('design-dataviz', 'Une visualisation accessible'),
    ('design-dataviz', 'Une exploration interactive'),
    ('design-ux-writing', 'Réécrire dix messages d''erreur'),
    ('design-ux-writing', 'Des états vides qui servent'),
    ('design-ux-writing', 'Un premier lancement'),
    ('design-ux-writing', 'Un guide de style éditorial'),
    ('design-ux-writing', 'Des textes qui survivent à la traduction'),
    ('design-marketing', 'Une campagne sur trois supports'),
    ('design-marketing', 'Un système de gabarits sociaux'),
    ('design-marketing', 'Un e-mail qui tient partout'),
    ('design-marketing', 'Une présentation qui se tient sans orateur'),
    ('design-marketing', 'Deux variantes testables'),
    ('design-game-ui', 'Un HUD lisible en action'),
    ('design-game-ui', 'Un menu navigable à la manette'),
    ('design-game-ui', 'Une interface diégétique'),
    ('design-game-ui', 'Des retours qui donnent de la sensation'),
    ('design-game-ui', 'Des options de confort'),
    ('design-game-environment', 'Des recherches de décor'),
    ('design-game-environment', 'Un kit modulaire'),
    ('design-game-environment', 'Une planche de matériaux'),
    ('design-game-environment', 'Un décor sous budget'),
    ('design-game-environment', 'Un langage visuel de monde'),
    ('design-arch-interior-viz', 'Une vue extérieure crédible'),
    ('design-arch-interior-viz', 'Un intérieur en lumière naturelle'),
    ('design-arch-interior-viz', 'Des matériaux avec leurs défauts'),
    ('design-arch-interior-viz', 'Une mise en scène habitée'),
    ('design-arch-interior-viz', 'Un travelling architectural'),
    ('design-ar-vr-spatial', 'Une interaction à la main'),
    ('design-ar-vr-spatial', 'Un confort mesuré'),
    ('design-ar-vr-spatial', 'Un ancrage en réalité augmentée'),
    ('design-ar-vr-spatial', 'De la typographie dans l''espace'),
    ('design-ar-vr-spatial', 'Une entrée en expérience'),
    ('design-sound', 'Une palette de sons d''interface'),
    ('design-sound', 'Une ambiance en couches'),
    ('design-sound', 'Du bruitage enregistré'),
    ('design-sound', 'Une identité sonore'),
    ('design-sound', 'Une livraison aux normes'),
    ('design-service', 'Un blueprint de service'),
    ('design-service', 'Une carte des parties prenantes'),
    ('design-service', 'Un parcours multi-canal'),
    ('design-service', 'Un atelier de co-conception'),
    ('design-service', 'Un prototype de service'),
    ('design-ops', 'Une convention de fichiers'),
    ('design-ops', 'Un passage au développement'),
    ('design-ops', 'Un rituel de revue'),
    ('design-ops', 'Des droits d''usage au clair'),
    ('design-ops', 'Mesurer un effet de conception')
  ) AS seed(orientation_slug, title)
  JOIN orientations o ON o.slug = seed.orientation_slug
 WHERE ct.title = seed.title
   AND ct.skill_domain = 'design'
   AND ct.orientation_id IS NULL;

-- ═══════════════════════════════════════════════════════════════════
-- Every trade got its five back
-- ═══════════════════════════════════════════════════════════════════
--
-- The same guard 0239 ends with, for the same reason: this UPDATE matches on
-- a title, and a title that has since been edited matches nothing. Silently
-- leaving a trade unattached would put us back where we started, with an
-- authoring screen that shows an empty orientation and no way to tell an
-- unwritten catalogue from a broken join.

DO $$
DECLARE
    attached INT;
    trades INT;
BEGIN
    SELECT count(*), count(DISTINCT orientation_id)
      INTO attached, trades
      FROM challenge_templates
     WHERE skill_domain = 'design' AND orientation_id IS NOT NULL;

    IF trades < 26 THEN
        RAISE EXCEPTION
            'only % of 26 design trades have challenges attached (% rows). A '
            'title edited since 0239 matches nothing here, and a trade with no '
            'challenges is invisible to the authoring surface.',
            trades, attached;
    END IF;
END $$;
