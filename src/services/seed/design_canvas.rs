//! Design work on our own surfaces.
//!
//! ## Why this is deliberately a minority of the design work
//!
//! Fifteen per cent, roughly. A platform whose designers only ever work on the
//! platform produces a portfolio nobody outside can read, and a community
//! whose only client is us is a community talking to itself. The partner
//! repositories and the curated briefs are where most of it should come from.
//!
//! What our own surfaces are good for is the first challenge: the brief is
//! short, the context is public, and a designer can see the thing they are
//! redesigning without asking anybody for access.
//!
//! ## Idempotent
//!
//! Keyed on the slice title inside its project. Running it twice updates the
//! brief and leaves the status alone -- so a challenge somebody has already
//! claimed is not yanked back to `open` by a deployment.

use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;

/// One piece of design work on one of our surfaces.
struct CanvasBrief {
    /// Which of our repositories it belongs to, by project slug. The project
    /// is created by `skilluv-seed-projects`; a brief whose project is absent
    /// is reported rather than skipped.
    project_slug: &'static str,
    title: &'static str,
    /// The brief. Written to the common structure of
    /// `docs/design/BRIEF-TEMPLATES.md`: context, problem, constraints,
    /// deliverables.
    brief: &'static str,
    orientation: &'static str,
    subtype: &'static str,
    difficulty: i16,
    estimated_hours: i32,
    expected_rounds: i16,
    /// True for the three the backlog marks as contests: work wide enough that
    /// several answers are worth comparing.
    contest: bool,
}

const BRIEFS: &[CanvasBrief] = &[
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Refonte de la page de vérification d'attestation",
        brief: "## Contexte\n\n\
            `/verify/{code}` est la page qu'un recruteur ouvre quand il veut savoir si une \
            attestation est vraie. C'est la seule page de la plateforme dont le lecteur n'est pas \
            un membre de la communauté, et souvent la première qu'il voit.\n\n\
            ## Problème\n\n\
            Elle affiche aujourd'hui un statut et une liste de champs. Un recruteur pressé n'y \
            trouve pas la réponse à sa question — « est-ce que cette personne a vraiment fait ce \
            travail » — et repart sans avoir cliqué sur la preuve.\n\n\
            ## Contraintes\n\n\
            Aucune authentification : la page doit se comprendre par quelqu'un qui ne connaît pas \
            Skilluv. Elle doit fonctionner sans JavaScript pour l'essentiel, et être imprimable — \
            une attestation finit parfois dans un dossier papier. Contraste AA obligatoire.\n\n\
            ## Livrables\n\n\
            Maquettes desktop et mobile des trois états (valide, révoquée, introuvable), la \
            version imprimable, et les valeurs de contraste vérifiées.",
        orientation: "design-product",
        subtype: "interface",
        difficulty: 3,
        estimated_hours: 12,
        expected_rounds: 3,
        contest: false,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "L'état vide de la liste de challenges",
        brief: "## Contexte\n\n\
            Quand rien ne correspond aux métiers déclarés par quelqu'un, `/challenges` affiche une \
            liste vide. C'est l'écran qu'un nouveau venu a le plus de chances de rencontrer les \
            premières semaines, quand le catalogue est encore mince.\n\n\
            ## Problème\n\n\
            Un vide sans explication se lit comme « cette plateforme n'a rien » alors qu'il veut \
            dire « rien ne correspond encore à ce que tu as déclaré ». La différence décide si la \
            personne revient.\n\n\
            ## Contraintes\n\n\
            L'illustration doit tenir en SVG léger et rester lisible en 320 px de large. Le texte \
            doit dire quoi faire ensuite — élargir ses métiers, proposer un brief, regarder les \
            concours — sans culpabiliser.\n\n\
            ## Livrables\n\n\
            L'illustration en SVG avec ses sources, les trois variantes de message, et la version \
            en mode sombre.",
        orientation: "design-illustration",
        subtype: "illustration_set",
        difficulty: 2,
        estimated_hours: 8,
        expected_rounds: 2,
        contest: false,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Le parcours d'arrivée d'un nouveau membre",
        brief: "## Contexte\n\n\
            À l'inscription, sept questions décident de ce qu'on montre à quelqu'un. Aucune n'est \
            une preuve : elles trient l'affichage, jamais le crédit.\n\n\
            ## Problème\n\n\
            Sept questions d'affilée ressemblent à un formulaire administratif, et un formulaire \
            administratif se remplit au hasard. Des réponses au hasard trient mal, et la personne \
            conclut que les recommandations ne valent rien.\n\n\
            ## Contraintes\n\n\
            Quatre écrans au maximum. Chaque question doit dire à quoi elle sert. On doit pouvoir \
            en sauter une sans se sentir en faute, et revenir dessus plus tard. Fonctionne sur un \
            écran de 5 pouces et sur une connexion lente.\n\n\
            ## Livrables\n\n\
            Le parcours complet avec ses états (vide, erreur, sauté, repris), et le texte de \
            chaque question — c'est le texte qui fait le travail ici, pas la mise en page.",
        orientation: "design-product",
        subtype: "interface",
        difficulty: 3,
        estimated_hours: 16,
        expected_rounds: 3,
        contest: false,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Le tableau de bord « mes challenges »",
        brief: "## Contexte\n\n\
            La page où quelqu'un voit ce qu'il a en cours, ce qu'il a validé, et où il en est sur \
            son échelle de métier.\n\n\
            ## Problème\n\n\
            Elle liste aujourd'hui des lignes. Elle ne montre pas la seule chose que la plateforme \
            a de particulier : la distance parcourue — un travail validé après trois tours de \
            critique compte plus qu'un validé du premier coup, et ça ne se voit nulle part.\n\n\
            ## Contraintes\n\n\
            Le progrès doit se lire en trois secondes, sans que ça devienne un jeu de barres à \
            remplir. Pas de chiffre inventé : tout ce qui est affiché existe en base. Lisible sans \
            couleur.\n\n\
            ## Livrables\n\n\
            La page, ses états (aucun challenge, un en cours, dix validés), et la façon dont les \
            tours de critique apparaissent.",
        orientation: "design-product",
        subtype: "interface",
        difficulty: 4,
        estimated_hours: 20,
        expected_rounds: 3,
        contest: false,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "La page d'entrée pour les mainteneurs de projets libres",
        brief: "## Contexte\n\n\
            Une page qui s'adresse aux mainteneurs de projets libres : pourquoi laisser Skilluv \
            envoyer des contributeurs sur leurs dépôts.\n\n\
            ## Problème\n\n\
            Un mainteneur a déjà trop de notifications et pas assez de temps. La page doit \
            répondre en dix secondes à « qu'est-ce que ça me coûte » avant de dire ce que ça \
            apporte.\n\n\
            ## Contraintes\n\n\
            Une page, pas un tunnel. Le lecteur est technique et se méfie du marketing : pas de \
            superlatif, des faits. Doit rester lisible avec les images bloquées.\n\n\
            ## Livrables\n\n\
            La page complète, la hiérarchie de lecture justifiée, et la version mobile.",
        orientation: "design-web",
        subtype: "interface",
        difficulty: 3,
        estimated_hours: 14,
        expected_rounds: 2,
        contest: false,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Composition de la page de confiance",
        brief: "## Contexte\n\n\
            Une page qui rassemble ce qui rend les preuves de la plateforme vérifiables : comment \
            une attestation est émise, ce qui peut la révoquer, ce que la plateforme ne fait pas.\n\n\
            ## Problème\n\n\
            Ces informations existent, éparpillées dans des documents que personne ne lit. Une \
            page qui les compose est ce qui permet à un recruteur de décider s'il fait confiance \
            au reste.\n\n\
            ## Contraintes\n\n\
            Le contenu est dense et juridique par endroits : le travail est de le rendre \
            parcourable sans le simplifier au point de le rendre faux. Illustration sobre — une \
            page de confiance qui a l'air d'une brochure perd son objet.\n\n\
            ## Livrables\n\n\
            La composition, le système de navigation interne, et les éléments graphiques en SVG.",
        orientation: "design-web",
        subtype: "interface",
        difficulty: 4,
        estimated_hours: 18,
        expected_rounds: 3,
        contest: true,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Le PDF d'attestation",
        brief: "## Contexte\n\n\
            Une attestation se télécharge en PDF. C'est le document qu'une personne joint à une \
            candidature, et parfois imprime.\n\n\
            ## Problème\n\n\
            Le PDF actuel est une transposition de la page web. Il ne tient pas debout comme \
            document : pas de hiérarchie propre à l'imprimé, un code de vérification qu'on ne \
            remarque pas, une typographie qui n'appartient à personne.\n\n\
            ## Contraintes\n\n\
            A4 et Letter. Une seule page. Lisible en noir et blanc et à la photocopie. Le code de \
            vérification doit être trouvable en une seconde par quelqu'un qui veut le saisir. \
            Polices libres de droits uniquement — une licence bureau ne se livre pas.\n\n\
            ## Livrables\n\n\
            Les gabarits pour les trois types d'attestation, les polices retenues avec leur \
            licence, et une épreuve imprimée photographiée.",
        orientation: "design-typography",
        subtype: "brand_kit",
        difficulty: 4,
        estimated_hours: 24,
        expected_rounds: 3,
        contest: true,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Système d'icônes Skilluv, première version",
        brief: "## Contexte\n\n\
            L'interface emprunte aujourd'hui ses icônes à une bibliothèque générique. Elles \
            fonctionnent et n'appartiennent à personne.\n\n\
            ## Problème\n\n\
            Certains objets de la plateforme n'ont pas d'icône qui leur corresponde : une \
            attestation, un tour de critique, un métier, un fragment. Les approximations actuelles \
            désignent autre chose.\n\n\
            ## Contraintes\n\n\
            Quarante icônes, une grille tenue, une seule épaisseur de trait, lisibles à 16 px — \
            c'est la taille qui décide, pas la vignette de présentation. Livrées en SVG optimisés \
            avec un viewBox commun.\n\n\
            ## Livrables\n\n\
            Les quarante SVG, la grille et les règles de construction, et une planche de contrôle \
            à 16 px.",
        orientation: "design-iconography",
        subtype: "icon_set",
        difficulty: 4,
        estimated_hours: 30,
        expected_rounds: 3,
        contest: false,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Design system Skilluv, première version",
        brief: "## Contexte\n\n\
            Le frontend et l'interface d'administration partagent une charte implicite et aucun \
            système. Les mêmes composants existent en deux variantes selon qui les a écrits.\n\n\
            ## Problème\n\n\
            Sans jetons ni composants documentés, chaque écran nouveau rouvre les mêmes questions, \
            et l'accessibilité se joue à chaque fois — donc se perd une fois sur trois.\n\n\
            ## Contraintes\n\n\
            Les jetons doivent être exportables en JSON exploitable par le frontend. Le pas \
            d'espacement et l'échelle typographique doivent être dérivés, pas listés à la main. \
            Chaque composant documente ses états, y compris l'erreur et le focus clavier.\n\n\
            ## Livrables\n\n\
            Les jetons en JSON, les composants de base documentés avec leurs états, et une note \
            expliquant ce qui a été écarté.",
        orientation: "design-system",
        subtype: "design_system",
        difficulty: 5,
        estimated_hours: 60,
        expected_rounds: 4,
        contest: true,
    },
    CanvasBrief {
        project_slug: "skilluv-frontend",
        title: "Identité en mouvement",
        brief: "## Contexte\n\n\
            Trois moments de l'interface méritent un mouvement : l'attente, la validation d'un \
            travail, et le passage d'un rang.\n\n\
            ## Problème\n\n\
            Le premier est un indicateur générique, les deux autres n'ont rien. Or la validation \
            est le moment que toute la plateforme existe pour produire, et il passe sans que rien \
            ne le marque.\n\n\
            ## Contraintes\n\n\
            Livré en Lottie. Moins de soixante calques et moins de cinq secondes — au-delà, le \
            rendu coûte cher sur les téléphones d'entrée de gamme que beaucoup de nos \
            utilisateurs ont. Ce qui se passe quand la réduction de mouvement est activée fait \
            partie du livrable, pas d'une note en bas de page.\n\n\
            ## Livrables\n\n\
            Les trois animations en Lottie, le projet source, et la version réduite de chacune.",
        orientation: "design-motion-ui",
        subtype: "motion",
        difficulty: 4,
        estimated_hours: 24,
        expected_rounds: 3,
        contest: false,
    },
];

/// Seed the canvas. Returns what it did, for the ledger.
///
/// A brief whose project is missing is counted and reported rather than
/// skipped in silence: it means the `projects` step has not run, and a
/// partially seeded canvas that reports success is the kind of thing somebody
/// discovers a month later.
pub async fn run(db: &PgPool, owner_id: Uuid) -> Result<String, AppError> {
    let mut created = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;

    for brief in BRIEFS {
        let project: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM projects WHERE slug = $1")
            .bind(brief.project_slug)
            .fetch_optional(db)
            .await?;

        // Reported rather than skipped quietly: a brief whose project is
        // missing means `skilluv-seed-projects` has not run, and the whole
        // batch would otherwise look like it worked.
        let Some((project_id,)) = project else {
            tracing::warn!(
                project = brief.project_slug,
                title = brief.title,
                "project not seeded yet"
            );
            skipped += 1;
            continue;
        };

        let orientation: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM orientations
              WHERE slug = $1 AND primary_domain = 'design' AND is_archived = FALSE",
        )
        .bind(brief.orientation)
        .fetch_optional(db)
        .await?;

        let Some((orientation_id,)) = orientation else {
            return Err(AppError::Internal(format!(
                "orientation {} is not a live design trade - the catalogue and this seed disagree",
                brief.orientation
            )));
        };

        // Keyed on the title inside its project, and the status is deliberately
        // left alone on update: a challenge somebody has already claimed must
        // not be yanked back to `open` by a re-run.
        let existing: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM project_slices WHERE project_id = $1 AND title = $2")
                .bind(project_id)
                .bind(brief.title)
                .fetch_optional(db)
                .await?;

        match existing {
            Some((id,)) => {
                sqlx::query(
                    "UPDATE project_slices
                        SET description = $2, difficulty = $3, estimated_hours = $4,
                            design_subtype = $5, design_expected_rounds = $6,
                            orientation_id = $7, updated_at = NOW()
                      WHERE id = $1",
                )
                .bind(id)
                .bind(brief.brief)
                .bind(brief.difficulty)
                .bind(brief.estimated_hours)
                .bind(brief.subtype)
                .bind(brief.expected_rounds)
                .bind(orientation_id)
                .execute(db)
                .await?;
                updated += 1;
                tracing::info!(title = brief.title, "brief updated");
            }
            None => {
                sqlx::query(
                    "INSERT INTO project_slices
                        (project_id, slice_type, title, description, primary_domain, difficulty,
                         estimated_hours, status, design_subtype, design_expected_rounds,
                         orientation_id, created_by_user_id, ingested_from)
                     VALUES ($1, 'design_artifact', $2, $3, 'design', $4, $5, 'open', $6, $7, $8,
                             $9, 'manual')",
                )
                .bind(project_id)
                .bind(brief.title)
                .bind(brief.brief)
                .bind(brief.difficulty)
                .bind(brief.estimated_hours)
                .bind(brief.subtype)
                .bind(brief.expected_rounds)
                .bind(orientation_id)
                .bind(owner_id)
                .execute(db)
                .await?;
                created += 1;
                tracing::info!(title = brief.title, "brief created");
            }
        }
    }

    let contests = BRIEFS.iter().filter(|b| b.contest).count();
    if skipped > 0 {
        return Err(AppError::Internal(format!(
            "{skipped} design briefs had no project to land in - the `projects` step has not run"
        )));
    }
    Ok(format!(
        "{created} created, {updated} updated, {contests} recommended as contests"
    ))
}
