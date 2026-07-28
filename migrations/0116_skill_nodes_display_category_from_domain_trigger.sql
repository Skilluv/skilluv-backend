-- Auto-derive skill_nodes.display_category from domain on INSERT.
--
-- Migration 0091 introduced the column with DEFAULT 'craft' and a one-shot
-- UPDATE that mapped every existing row from `domain` to a category. New
-- skills inserted afterwards (0105 firmware-security, and any future
-- content-strategy seed) silently kept the 'craft' default even when their
-- domain called for 'operate' / 'understand' / etc. Test
-- `security_and_ops_domains_map_to_operate` caught the mismatch.
--
-- Fix:
--   1. Backfill: re-apply the 0091 mapping for anything still on the default.
--   2. Trigger BEFORE INSERT that fills the mapping when the caller doesn't
--      explicitly override — so this class of drift can't recur silently.

CREATE OR REPLACE FUNCTION skill_nodes_default_display_category(_domain VARCHAR)
RETURNS VARCHAR AS $$
BEGIN
    RETURN CASE _domain
        WHEN 'code'        THEN 'craft'
        WHEN 'design'      THEN 'create'
        WHEN 'game'        THEN 'create'
        WHEN 'security'    THEN 'operate'
        WHEN 'ops'         THEN 'operate'
        WHEN 'soft_skills' THEN 'share'
        WHEN 'ai'          THEN 'understand'
        ELSE 'craft'
    END;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Backfill: any row where display_category doesn't match its domain-derived
-- default is likely a post-0091 seed that missed the mapping. We only touch
-- 'craft' rows to avoid clobbering the manual 'meta' promotions from 0091.
UPDATE skill_nodes
SET display_category = skill_nodes_default_display_category(domain)
WHERE display_category = 'craft'
  AND skill_nodes_default_display_category(domain) <> 'craft';

CREATE OR REPLACE FUNCTION skill_nodes_set_display_category()
RETURNS TRIGGER AS $$
BEGIN
    -- Only fill when the caller didn't set it explicitly (or left it at the
    -- default). We can't distinguish "unset" from "explicit 'craft'", so the
    -- rule is: if the value ends up as 'craft' for a non-code domain, we
    -- assume it was implicit and remap.
    IF NEW.display_category = 'craft'
       AND skill_nodes_default_display_category(NEW.domain) <> 'craft' THEN
        NEW.display_category := skill_nodes_default_display_category(NEW.domain);
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_skill_nodes_default_display_category ON skill_nodes;
CREATE TRIGGER trg_skill_nodes_default_display_category
    BEFORE INSERT ON skill_nodes
    FOR EACH ROW
    EXECUTE FUNCTION skill_nodes_set_display_category();
