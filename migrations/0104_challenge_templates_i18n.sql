-- Content strategy foundation — bilingue FR/EN dès Saison 1 "Hello World" 2027,
-- extension future à AR (Saison Mukwege 2028) + langues locales (wolof, lingala,
-- swahili) sans nouvelle migration DB.
--
-- Rationale décision H de la stratégie contenu 2027-2028 :
--   Le modèle JSONB {locale: text} est retenu contre :
--   - Colonnes title_fr/title_en/... (rigide, migration à chaque nouvelle langue)
--   - Table translation dédiée (norme mais lourde pour un champ court)
--
--   Contrainte applicative (à enforcer côté route API) : au minimum une des clés
--   'fr' ou 'en' doit être présente et non vide. Rejet 400 sinon.
--
-- Backfill : les valeurs actuelles title/description (VARCHAR/TEXT) sont
-- transposées vers title_i18n->>'fr' et description_i18n->>'fr' (français par
-- défaut, hérité du seed initial 0003 en français).
--
-- Colonnes legacy title/description CONSERVÉES le temps que le frontend +
-- l'admin lisent les nouvelles colonnes i18n. DROP prévu en migration future
-- une fois le code migré (voir P26.x).

ALTER TABLE challenge_templates
    ADD COLUMN title_i18n JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN description_i18n JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN instructions_i18n JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Backfill depuis les colonnes actuelles (français = locale d'origine du seed).
UPDATE challenge_templates
SET title_i18n = jsonb_build_object('fr', title),
    description_i18n = jsonb_build_object('fr', description),
    instructions_i18n = jsonb_build_object('fr', instructions)
WHERE title_i18n = '{}'::jsonb;

-- Contrainte : au moins une clé 'fr' OU 'en' non-vide dans title_i18n.
-- Description + instructions restent plus tolérantes (peuvent être omises pour
-- des micro-quêtes ultra-courtes qui tiennent dans le titre).
ALTER TABLE challenge_templates
    ADD CONSTRAINT challenge_templates_title_i18n_min_locale
    CHECK (
        (title_i18n ? 'fr' AND length(title_i18n->>'fr') > 0)
        OR (title_i18n ? 'en' AND length(title_i18n->>'en') > 0)
    );

-- Index GIN pour recherche full-text future sur les 3 champs i18n
-- (les micro-quêtes contribuables devront être searchables par mot-clé multi-langue).
CREATE INDEX idx_challenge_templates_title_i18n_gin
    ON challenge_templates USING gin (title_i18n);

CREATE INDEX idx_challenge_templates_description_i18n_gin
    ON challenge_templates USING gin (description_i18n);

COMMENT ON COLUMN challenge_templates.title_i18n IS
    'Titre bilingue+ format {locale: text}. Locales officielles 2027 : fr, en. Ajoutées 2028+ : ar. Ajoutées 2029+ via partenariats : wo, ln, sw, ff, am.';
COMMENT ON COLUMN challenge_templates.description_i18n IS
    'Description bilingue+ ; peut être vide pour micro-quêtes triviales.';
COMMENT ON COLUMN challenge_templates.instructions_i18n IS
    'Instructions pas-à-pas bilingues+ ; format markdown.';
