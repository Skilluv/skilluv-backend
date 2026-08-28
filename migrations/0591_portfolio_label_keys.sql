-- Portfolio labels get a language-neutral key.
--
-- `items_label` / `reach_label` were seeded as display words, and inconsistently
-- — some English ('downloads', 'stars'), some French ('téléchargements',
-- 'étoiles', 'dépôts'). They come out of the public GET /portfolio-platforms
-- verbatim, so a French word reaches an English reader with no way for the
-- client to fix it (SKI-311). The fix is Option A: a stable key the client
-- translates, added alongside the display label rather than replacing it, so
-- nothing breaks mid-transition.

ALTER TABLE portfolio_platforms
    ADD COLUMN items_label_key VARCHAR(40),
    ADD COLUMN reach_label_key VARCHAR(40);

-- Map the French display words to canonical English keys; everything already
-- English becomes its own key (lowercased, spaces to underscores), so
-- 'disclosed reports' -> 'disclosed_reports' and 'stars' -> 'stars'.
UPDATE portfolio_platforms SET
    items_label_key = CASE items_label
        WHEN 'téléchargements' THEN 'downloads'
        WHEN 'paquets'         THEN 'packages'
        WHEN 'dépôts'          THEN 'repositories'
        WHEN 'morceaux'        THEN 'tracks'
        WHEN 'modèles'         THEN 'models'
        WHEN 'rôles'           THEN 'roles'
        WHEN 'écoutes'         THEN 'plays'
        WHEN 'étoiles'         THEN 'stars'
        ELSE lower(replace(items_label, ' ', '_'))
    END,
    reach_label_key = CASE reach_label
        WHEN 'téléchargements' THEN 'downloads'
        WHEN 'paquets'         THEN 'packages'
        WHEN 'dépôts'          THEN 'repositories'
        WHEN 'morceaux'        THEN 'tracks'
        WHEN 'modèles'         THEN 'models'
        WHEN 'rôles'           THEN 'roles'
        WHEN 'écoutes'         THEN 'plays'
        WHEN 'étoiles'         THEN 'stars'
        ELSE lower(replace(reach_label, ' ', '_'))
    END;

-- No natural-language French left in a key: the whole point is a code an
-- English client can render. If a label was added later and missed above, this
-- fails loudly rather than shipping a French key.
DO $$
DECLARE bad INT;
BEGIN
    SELECT count(*) INTO bad FROM portfolio_platforms
     WHERE items_label_key ~ '[éèêàùçôîïâûœ]'
        OR reach_label_key ~ '[éèêàùçôîïâûœ]'
        OR items_label_key IN ('paquets','morceaux','depots','roles_fr')
        OR reach_label_key IN ('paquets','morceaux');
    IF bad > 0 THEN
        RAISE EXCEPTION 'a portfolio label key still reads as French — % row(s)', bad;
    END IF;
END $$;
