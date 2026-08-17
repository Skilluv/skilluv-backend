-- What a reviewer looks at, per family of AI trade.
--
-- Migration 0180 built the table and gave `code` its grids. Until this one,
-- `ai` had no default: a challenge in the AI domain with no hand-written
-- rubric was sent to the verifier with its instructions alone — the model was
-- asked whether the work was good with no statement of what good means, and
-- answered anyway.
--
-- ## The common criteria are not the code ones
--
-- Correctness is the wrong first question for a model. A classifier at 94%
-- is not "correct" or "incorrect"; the questions that decide whether the work
-- is worth anything are whether the evaluation was honest, whether somebody
-- else can re-run it, and where the data came from. Those three sit at the
-- top of the domain grid for that reason.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('ai', NULL, 'IA — critères communs', '[
  {"criterion": "Honnêteté de l''évaluation", "looks_like": "Le jeu de test est séparé de l''entraînement et ressemble à la production. Pas de fuite de données, et le score annoncé est celui du jeu de test, pas du meilleur essai."},
  {"criterion": "Reproductibilité", "looks_like": "Graines, versions de bibliothèques et données figées. Un lecteur relance et retrouve les mêmes chiffres, ou l''écart attendu est écrit."},
  {"criterion": "Provenance des données", "looks_like": "D''où viennent les données, sous quelle licence, avec quel consentement. Un jeu aspiré sans droit rend tout le reste inutilisable."},
  {"criterion": "Limites énoncées", "looks_like": "Ce sur quoi le modèle échoue est écrit par l''auteur, pas découvert par le relecteur. Un travail qui ne connaît pas ses limites n''a pas été évalué."},
  {"criterion": "Documentation", "looks_like": "Un lecteur sait quoi lancer, sur quel matériel, et pourquoi les choix ont été faits. Un travail sans documentation est refusé."},
  {"criterion": "Transparence sur l''IA", "looks_like": "L''usage d''un assistant est déclaré. Il est accepté ; le camoufler ne l''est pas."}
]'),

('ai', 'data', 'Données — grille de revue', '[
  {"criterion": "Fiabilité du pipeline", "looks_like": "Les reprises sont idempotentes, un rattrapage ne duplique rien, et l''échec d''une étape ne laisse pas la table à moitié écrite."},
  {"criterion": "Qualité des données", "looks_like": "Des contrôles s''exécutent à chaque passage — unicité, fraîcheur, plages de valeurs — et bloquent en amont du tableau de bord plutôt qu''en aval."},
  {"criterion": "Passage à l''échelle", "looks_like": "Mesuré sur un volume réaliste, pas sur l''échantillon. Le comportement au-delà est énoncé."},
  {"criterion": "Coût", "looks_like": "Octets scannés, fréquence et rétention sont connus. Une requête planifiée coûte tous les jours."},
  {"criterion": "Définition des métriques", "looks_like": "Chaque chiffre affiché a une définition écrite que deux personnes calculeraient pareil."},
  {"criterion": "Documentation", "looks_like": "Le schéma, les sources et les hypothèses sont écrits. Un lecteur sait ce que la colonne veut dire."}
]'),

('ai', 'ml', 'Modèles — grille de revue', '[
  {"criterion": "Performance", "looks_like": "Comparée à une base de référence, pas dans l''absolu. Un modèle sans témoin ne prouve rien."},
  {"criterion": "Généralisation", "looks_like": "Testé hors du jeu d''entraînement, et sur une distribution qui a bougé si c''est possible. Pas de fuite temporelle."},
  {"criterion": "Reproductibilité", "looks_like": "Le code d''entraînement, les graines et les versions permettent de refaire le résultat."},
  {"criterion": "Interprétabilité", "looks_like": "On sait sur quoi le modèle s''appuie, au moins grossièrement. Un modèle qu''on ne peut pas interroger ne se débogue pas."},
  {"criterion": "Prêt à servir", "looks_like": "Latence, mémoire et format d''export mesurés. Un modèle qui ne tient pas dans la cible n''est pas fini."},
  {"criterion": "Surveillance", "looks_like": "Ce qui sera surveillé une fois en production est décidé avant le déploiement, pas après le premier incident."}
]'),

('ai', 'llm-nlp', 'Langage — grille de revue', '[
  {"criterion": "Rigueur des invites", "looks_like": "Les invites sont versionnées et testées. Une modification est justifiée par une mesure, pas par une impression."},
  {"criterion": "Méthode d''évaluation", "looks_like": "Un jeu d''évaluation existe, avec des cas d''échec choisis exprès. Trois exemples réussis ne sont pas une évaluation."},
  {"criterion": "Garde-fous", "looks_like": "Injection, fuite de données et sorties nuisibles sont traitées explicitement, et l''attaque correspondante est testée."},
  {"criterion": "Coût et latence", "looks_like": "Jetons par requête et temps de réponse mesurés. Le compromis qualité/coût est un choix documenté."},
  {"criterion": "Comportement au bord", "looks_like": "Ce que le système fait quand il ne sait pas : dire qu''il ne sait pas, ou inventer. Le premier est le seul acceptable."},
  {"criterion": "Documentation", "looks_like": "Le lecteur peut rejouer une conversation et comprendre pourquoi la réponse est celle-là."}
]'),

('ai', 'cv', 'Vision — grille de revue', '[
  {"criterion": "Exactitude", "looks_like": "Métriques adaptées à la tâche — mAP, mIoU — mesurées sur un jeu de test que l''auteur n''a pas ajusté."},
  {"criterion": "Robustesse", "looks_like": "Testé sur des conditions dégradées : lumière, flou, occlusion, angle. Les images propres ne prouvent rien."},
  {"criterion": "Diversité du jeu de données", "looks_like": "Composition connue et écrite. La performance par sous-population est mesurée, pas supposée uniforme."},
  {"criterion": "Éthique", "looks_like": "Consentement et usage prévu sont énoncés. Un modèle qui reconnaît des personnes est traité comme tel."},
  {"criterion": "Performance à l''inférence", "looks_like": "Images par seconde et mémoire mesurées sur le matériel visé, pas sur une carte de centre de données."},
  {"criterion": "Documentation", "looks_like": "Provenance des images, annotations, licences. Un jeu d''images sans licence n''est pas publiable."}
]'),

('ai', 'safety', 'Sûreté — grille de revue', '[
  {"criterion": "Rigueur du protocole", "looks_like": "L''attaque est décrite assez précisément pour être rejouée par un tiers, avec le modèle et la version exacts."},
  {"criterion": "Reproductibilité", "looks_like": "Un taux de succès sur N tentatives, pas une capture d''écran. Une trouvaille non reproductible n''est pas une trouvaille."},
  {"criterion": "Nouveauté", "looks_like": "Ce que ce travail ajoute à ce qui est déjà publié est dit, et les travaux voisins sont cités."},
  {"criterion": "Recommandations exploitables", "looks_like": "Une atténuation concrète est proposée. Signaler sans proposer laisse le problème entier."},
  {"criterion": "Divulgation", "looks_like": "L''éditeur a été prévenu, un délai a été convenu, et la publication le respecte."},
  {"criterion": "Double usage", "looks_like": "Ce qui est retenu et pourquoi. Publier l''intégralité d''une attaque n''est pas toujours le choix responsable."}
]');
