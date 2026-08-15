-- What a reviewer looks at, per family of trade.
--
-- ## Why data and not five markdown files
--
-- A grid in a document is read once and then remembered wrongly. This one is
-- read by two things: a human opening a review, and the LLM verifier, which
-- today receives `challenge_templates.evaluation_rubric` and nothing else.
--
-- A challenge with no rubric is verified against its instructions alone —
-- the model is asked whether the work is good with no statement of what good
-- means. Every challenge created without someone hand-writing a rubric is in
-- that state. A family grid gives it criteria to fall back on.
--
-- ## Keying
--
-- `(domain, reviewer_group)` with a NULL group meaning "anything in this
-- domain". A challenge carries a domain but not a trade, so the fallback
-- resolves on the domain; a human reviewing a specific orientation gets the
-- sharper grid.

CREATE TABLE review_grids (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain VARCHAR(30) NOT NULL,
    -- NULL is the domain's default, used when nothing narrower applies.
    reviewer_group VARCHAR(30),
    display_name VARCHAR(120) NOT NULL,
    -- Ordered list of {"criterion": ..., "looks_like": ...} objects.
    criteria JSONB NOT NULL,
    admin_editable BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT review_grids_criteria_is_a_list
        CHECK (jsonb_typeof(criteria) = 'array' AND jsonb_array_length(criteria) > 0)
);

-- One grid per family, and one default per domain.
CREATE UNIQUE INDEX uniq_review_grids_group
    ON review_grids (domain, reviewer_group)
    WHERE reviewer_group IS NOT NULL;

CREATE UNIQUE INDEX uniq_review_grids_domain_default
    ON review_grids (domain)
    WHERE reviewer_group IS NULL;

COMMENT ON TABLE review_grids IS
    'Review criteria per family of trade. Read by human reviewers and used '
    'as the fallback rubric when a challenge carries none, so verification '
    'never runs with no statement of what good means.';

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('code', NULL, 'Code — critères communs', '[
  {"criterion": "Correction", "looks_like": "Le code fait ce que l''énoncé demande, y compris aux bords : entrée vide, valeur limite, erreur réseau."},
  {"criterion": "Tests", "looks_like": "Les tests décrivent le comportement attendu, pas l''implémentation. Un test qui casse quand on refactore sans changer le comportement est un mauvais test."},
  {"criterion": "Documentation", "looks_like": "Un lecteur qui découvre le dépôt sait quoi lancer et pourquoi les choix ont été faits. Un code sans documentation est refusé."},
  {"criterion": "Lisibilité", "looks_like": "Les noms disent l''intention. Le découpage suit le problème, pas la facilité d''écriture."},
  {"criterion": "Transparence sur l''IA", "looks_like": "L''usage d''un assistant est déclaré. Il est accepté ; le camoufler ne l''est pas."}
]'),

('code', 'web', 'Web — grille de revue', '[
  {"criterion": "Responsive", "looks_like": "Utilisable au clavier sur un écran de téléphone comme sur un grand écran, sans défilement horizontal."},
  {"criterion": "Performance", "looks_like": "Core Web Vitals mesurés avant et après, pas estimés. Le budget de bundle est tenu."},
  {"criterion": "Accessibilité", "looks_like": "Navigable au clavier, contrastes conformes, sémantique HTML correcte. Testé, pas supposé."},
  {"criterion": "Référencement", "looks_like": "Titres, métadonnées et rendu serveur cohérents avec ce que la page prétend être."},
  {"criterion": "Couverture de test", "looks_like": "Les parcours critiques ont un test de bout en bout."},
  {"criterion": "Qualité du code", "looks_like": "Pas d''état global implicite, pas de duplication qu''un nom aurait évitée."}
]'),

('code', 'mobile', 'Mobile — grille de revue', '[
  {"criterion": "Conventions de la plateforme", "looks_like": "Navigation, retour, permissions suivent iOS ou Android, pas une traduction du web."},
  {"criterion": "Performance", "looks_like": "Démarrage à froid mesuré, pas d''image sautée au défilement."},
  {"criterion": "Batterie et réseau", "looks_like": "Pas de réveil inutile, pas de requête en boucle sur réseau instable."},
  {"criterion": "Hors-ligne", "looks_like": "L''application reste utilisable sans réseau et réconcilie sans perdre de données."},
  {"criterion": "Couverture de test", "looks_like": "La logique métier est testée hors interface."},
  {"criterion": "Accessibilité mobile", "looks_like": "Compatible avec le lecteur d''écran et les tailles de police système."}
]'),

('code', 'systems', 'Systèmes et bas niveau — grille de revue', '[
  {"criterion": "Correction", "looks_like": "Les cas d''erreur sont traités, pas ignorés. Les valeurs de retour sont vérifiées."},
  {"criterion": "Performance", "looks_like": "Mesurée avec un profileur, pas déduite. Le chemin chaud est identifié."},
  {"criterion": "Sûreté mémoire", "looks_like": "Pas de fuite, pas d''usage après libération. L''absence est démontrée, pas affirmée."},
  {"criterion": "Documentation", "looks_like": "Les invariants et les contraintes de contexte d''appel sont écrits."},
  {"criterion": "Couverture de test", "looks_like": "Tests sur matériel ou en simulation, y compris les conditions de panne."},
  {"criterion": "Portabilité", "looks_like": "Les hypothèses sur l''architecture et le boutisme sont explicites."}
]'),

('code', 'blockchain', 'Blockchain — grille de revue', '[
  {"criterion": "Sécurité", "looks_like": "Réentrance, dépassements, contrôle d''accès et manipulation d''oracle examinés un par un."},
  {"criterion": "Coût en gas", "looks_like": "Le coût des opérations de stockage est mesuré et justifié."},
  {"criterion": "Composabilité", "looks_like": "Le contrat se comporte correctement quand un autre contrat l''appelle."},
  {"criterion": "Tests par fuzzing", "looks_like": "Propriétés vérifiées sur des entrées aléatoires, pas seulement sur des cas choisis."},
  {"criterion": "Documentation", "looks_like": "Les hypothèses de confiance et les pouvoirs de l''administrateur sont énoncés."},
  {"criterion": "Irréversibilité", "looks_like": "Ce qui ne peut plus être corrigé après déploiement est identifié avant."}
]'),

('code', 'compilers', 'Compilation et preuves — grille de revue', '[
  {"criterion": "Correction", "looks_like": "La transformation préserve la sémantique. Les cas limites de la grammaire sont couverts."},
  {"criterion": "Messages d''erreur", "looks_like": "Un message dit où, quoi et comment corriger. Un outil qui dit seulement non n''aide personne."},
  {"criterion": "Performance", "looks_like": "Le coût sur un fichier réel est mesuré, pas sur un exemple de trois lignes."},
  {"criterion": "Conception d''API", "looks_like": "Les structures de données exposées survivent à une extension du langage."},
  {"criterion": "Documentation", "looks_like": "Les choix de représentation sont expliqués, pas seulement décrits."},
  {"criterion": "Couverture de test", "looks_like": "Tests de propriétés sur des programmes générés."}
]'),

('code', 'data', 'Données et systèmes distribués — grille de revue', '[
  {"criterion": "Correction sous panne", "looks_like": "Le comportement est défini quand un nœud tombe au pire moment."},
  {"criterion": "Performance", "looks_like": "Mesurée sous charge et sur volume réaliste, avec les percentiles hauts."},
  {"criterion": "Durabilité", "looks_like": "Ce qui est confirmé au client survit à un redémarrage brutal."},
  {"criterion": "Conception d''API", "looks_like": "Les opérations sont idempotentes ou disent explicitement qu''elles ne le sont pas."},
  {"criterion": "Documentation", "looks_like": "Les garanties offertes sont écrites, y compris ce qu''elles ne couvrent pas."},
  {"criterion": "Couverture de test", "looks_like": "Injection de panne, pas seulement chemin nominal."}
]'),

('code', 'scientific', 'Calcul scientifique et GPU — grille de revue', '[
  {"criterion": "Correction numérique", "looks_like": "Stabilité et conditionnement examinés. L''erreur accumulée est bornée et connue."},
  {"criterion": "Reproductibilité", "looks_like": "Même entrée, même environnement, même résultat. Les graines et versions sont figées."},
  {"criterion": "Performance", "looks_like": "Profilée. Le goulot est identifié comme mémoire ou calcul, pas supposé."},
  {"criterion": "Conception d''API", "looks_like": "Les unités et les conventions de forme sont explicites."},
  {"criterion": "Documentation", "looks_like": "La méthode et ses limites de validité sont écrites."},
  {"criterion": "Couverture de test", "looks_like": "Comparaison à une solution analytique ou à une référence connue."}
]'),

('code', 'devtools-media', 'Outillage, média et applications — grille de revue', '[
  {"criterion": "Correction", "looks_like": "L''outil fait ce qu''il annonce sur des entrées réelles, pas seulement sur l''exemple du README."},
  {"criterion": "Ergonomie", "looks_like": "Les messages guident. Les codes de sortie sont exploitables par un script."},
  {"criterion": "Performance", "looks_like": "Mesurée sur un cas réaliste, avec la taille de données que les gens ont vraiment."},
  {"criterion": "Conception d''API", "looks_like": "Les options composent entre elles sans surprise."},
  {"criterion": "Documentation", "looks_like": "Installation, usage courant et limites connues sont écrits."},
  {"criterion": "Couverture de test", "looks_like": "Les chemins d''erreur sont testés autant que le chemin nominal."}
]');
