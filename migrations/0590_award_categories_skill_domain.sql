-- Award categories gain a skill_domain, so an awards page can show one family.
--
-- The front hit this building /design/awards (SKI-314): GET /awards/categories
-- returned every category on the platform with no way to tell which belongs to
-- which family, so a design page could not restrict itself to design awards.
--
-- Nullable, exactly as portfolio_platforms, mission_types and
-- tournaments.skill_domain carry it: NULL means the category is cross-cutting
-- (a platform-wide award, open to every domain), a real value scopes it to one
-- family. Purely additive — no existing category changes, and the seeding of
-- per-family categories (SKI-239 and its kin) is a separate ticket.

ALTER TABLE award_categories
    ADD COLUMN skill_domain VARCHAR(30);

COMMENT ON COLUMN award_categories.skill_domain IS
    'The family this award belongs to, or NULL for a cross-cutting one. Same '
    'convention as tournaments.skill_domain and portfolio_platforms.skill_domain.';

CREATE INDEX idx_award_categories_skill_domain
    ON award_categories (skill_domain) WHERE skill_domain IS NOT NULL;
