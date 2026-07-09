-- Pattern C onboarding : `skill_domain` peut désormais rester NULL le temps de l'onboarding post-SSO.
-- Le CHECK est ré-appliqué au niveau applicatif (validate_skill_domain).

ALTER TABLE users
    ALTER COLUMN skill_domain DROP NOT NULL;

-- Le CHECK original interdisait NULL implicitement. On le remplace par une version qui autorise NULL
-- mais garde l'énumération quand la valeur est présente.
ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_skill_domain_check;

ALTER TABLE users
    ADD CONSTRAINT users_skill_domain_check
    CHECK (skill_domain IS NULL OR skill_domain IN ('code', 'design', 'game', 'security'));
