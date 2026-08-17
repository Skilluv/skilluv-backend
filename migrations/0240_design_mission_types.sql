-- The kinds of paid design work.
--
-- Migration 0192 built missions, applications and billing for every domain,
-- keyed by `mission_types.skill_domain`. Design needs rows, not a mechanism —
-- and the backlog's `design_missions` table would have been a second copy of
-- everything 0192 already does, down to the escrow.
--
-- ## Why the list stops where it does
--
-- Twelve kinds, and each one is something a company has actually asked a
-- designer for. The temptation is to mirror the twenty-six trades, which
-- would produce a dropdown nobody reads and twenty-six near-empty listings.
-- A company hires for an outcome — an identity, an app, a film — and picks
-- the trade afterwards, which is what `missions.orientation_id` is for.
--
-- ## The one that is not a deliverable
--
-- `design_critique_session` buys someone's judgement for an afternoon, not an
-- artefact. It is here because it is the kind of work a senior designer can
-- take on between two projects, and because a marketplace that only sells
-- finished objects has nothing for the people who are best at reviewing them.

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order)
VALUES
    ('brand_identity_design', 'design', 'Identité de marque',
     'Logotype, palette, typographie, applications et les guidelines qui permettent à d''autres de les appliquer.', 10),

    ('product_interface_design', 'design', 'Interface produit',
     'Les écrans d''un produit, de la recherche au prototype remis au développement.', 20),

    ('website_design', 'design', 'Site web',
     'Un site complet, réalisable dans l''outil visé, jusqu''aux maquettes remises.', 30),

    ('mobile_app_design', 'design', 'Application mobile',
     'Les écrans d''une application native ou cross-platform, conventions de plateforme comprises.', 40),

    ('design_system_build', 'design', 'Design system',
     'Tokens, composants, documentation et le modèle de contribution qui le garde vivant.', 50),

    ('motion_production', 'design', 'Production motion',
     'Une séquence animée livrée dans son format de diffusion, avec ses sources.', 60),

    ('illustration_commission', 'design', 'Illustration commandée',
     'Une image ou une série, livrée avec ses sources et ses droits d''usage.', 70),

    ('iconography_set', 'design', 'Jeu d''icônes',
     'Un système d''icônes cohérent, livré dans un format intégrable sans retouche.', 80),

    ('dataviz_commission', 'design', 'Visualisation de données',
     'Un graphique, un tableau de bord ou une infographie, avec les définitions derrière chaque chiffre.', 90),

    ('ux_writing_pass', 'design', 'Passe de rédaction UX',
     'Reprendre les textes d''un produit : microcopie, erreurs, états vides, ton de voix.', 100),

    ('campaign_design', 'design', 'Campagne',
     'Une idée déclinée sur les supports demandés, dans les contraintes de chaque canal.', 110),

    ('design_critique_session', 'design', 'Séance de critique',
     'Le regard de quelqu''un d''expérimenté sur un travail en cours, avec une grille remplie et un compte rendu écrit.', 120)
ON CONFLICT (slug) DO NOTHING;
