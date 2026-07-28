-- Trigger BEFORE INSERT/UPDATE : si title_i18n (ou description_i18n / instructions_i18n)
-- est laisse a son default '{}'::jsonb alors que la colonne legacy title est
-- fournie, on auto-remplit avec {"fr": title}. Idem pour description/instructions.
--
-- Rationale : la constraint challenge_templates_title_i18n_min_locale (migration
-- 0104) refuse les rows avec title_i18n vide. Les tests d'integration existants
-- ecrivent la colonne legacy uniquement — plutot que de rewrite tous les tests,
-- on garantit ici l'invariant applicatif ("un challenge_template a toujours au
-- moins une locale") au niveau DB.
--
-- Le trigger ne touche pas title_i18n si deja fourni (idempotent). Meme logique
-- pour description_i18n et instructions_i18n meme s'ils n'ont pas de constraint,
-- pour rester consistant.

CREATE OR REPLACE FUNCTION challenge_templates_autofill_i18n()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.title_i18n = '{}'::jsonb AND NEW.title IS NOT NULL AND length(NEW.title) > 0 THEN
        NEW.title_i18n := jsonb_build_object('fr', NEW.title);
    END IF;
    IF NEW.description_i18n = '{}'::jsonb AND NEW.description IS NOT NULL AND length(NEW.description) > 0 THEN
        NEW.description_i18n := jsonb_build_object('fr', NEW.description);
    END IF;
    IF NEW.instructions_i18n = '{}'::jsonb AND NEW.instructions IS NOT NULL AND length(NEW.instructions) > 0 THEN
        NEW.instructions_i18n := jsonb_build_object('fr', NEW.instructions);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_challenge_templates_autofill_i18n ON challenge_templates;
CREATE TRIGGER trg_challenge_templates_autofill_i18n
    BEFORE INSERT OR UPDATE ON challenge_templates
    FOR EACH ROW
    EXECUTE FUNCTION challenge_templates_autofill_i18n();
