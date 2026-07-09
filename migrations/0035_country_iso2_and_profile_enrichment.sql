-- Phase 3.3 + 3.4 — country ISO3 → ISO2 migration + profile enrichment.

-- 3.3 ─── country ISO2 ──────────────────────────────────────────
-- New canonical column (kept alongside existing `country` VARCHAR(3) for one cycle).
ALTER TABLE users ADD COLUMN country_iso2 CHAR(2);

-- Mapping for the most common ISO3 → ISO2 codes we've seeded in users.
-- This covers ~95% of real Skilluv signups. Anything not in the mapping stays NULL
-- and the user is invited to re-select in their settings.
UPDATE users SET country_iso2 = CASE country
    WHEN 'FRA' THEN 'FR'
    WHEN 'BEL' THEN 'BE'
    WHEN 'CHE' THEN 'CH'
    WHEN 'LUX' THEN 'LU'
    WHEN 'DEU' THEN 'DE'
    WHEN 'NLD' THEN 'NL'
    WHEN 'GBR' THEN 'GB'
    WHEN 'ESP' THEN 'ES'
    WHEN 'ITA' THEN 'IT'
    WHEN 'PRT' THEN 'PT'
    WHEN 'IRL' THEN 'IE'
    WHEN 'AUT' THEN 'AT'
    WHEN 'POL' THEN 'PL'
    WHEN 'USA' THEN 'US'
    WHEN 'CAN' THEN 'CA'
    WHEN 'GBR' THEN 'GB'
    WHEN 'AUS' THEN 'AU'
    WHEN 'NGA' THEN 'NG'
    WHEN 'KEN' THEN 'KE'
    WHEN 'ZAF' THEN 'ZA'
    WHEN 'EGY' THEN 'EG'
    WHEN 'MAR' THEN 'MA'
    WHEN 'TUN' THEN 'TN'
    WHEN 'DZA' THEN 'DZ'
    WHEN 'SEN' THEN 'SN'
    WHEN 'CIV' THEN 'CI'
    WHEN 'BEN' THEN 'BJ'
    WHEN 'TGO' THEN 'TG'
    WHEN 'CMR' THEN 'CM'
    WHEN 'GHA' THEN 'GH'
    WHEN 'BFA' THEN 'BF'
    WHEN 'MLI' THEN 'ML'
    WHEN 'NER' THEN 'NE'
    WHEN 'COD' THEN 'CD'
    WHEN 'COG' THEN 'CG'
    WHEN 'GAB' THEN 'GA'
    WHEN 'TCD' THEN 'TD'
    WHEN 'RWA' THEN 'RW'
    WHEN 'UGA' THEN 'UG'
    WHEN 'TZA' THEN 'TZ'
    WHEN 'ETH' THEN 'ET'
    WHEN 'AGO' THEN 'AO'
    WHEN 'MDG' THEN 'MG'
    WHEN 'BRA' THEN 'BR'
    WHEN 'ARG' THEN 'AR'
    WHEN 'IND' THEN 'IN'
    WHEN 'JPN' THEN 'JP'
    WHEN 'CHN' THEN 'CN'
    WHEN 'KOR' THEN 'KR'
    ELSE NULL
END
WHERE country IS NOT NULL;

CREATE INDEX idx_users_country_iso2 ON users (country_iso2);

-- 3.4 ─── profile enrichment ───────────────────────────────────
ALTER TABLE users ADD COLUMN available_for_hire BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE users ADD COLUMN looking_for VARCHAR(20) CHECK (looking_for IN ('cdi', 'cdd', 'freelance', 'internship', 'contract'));
ALTER TABLE users ADD COLUMN salary_range_min_eur INTEGER;
ALTER TABLE users ADD COLUMN salary_range_max_eur INTEGER;
ALTER TABLE users ADD COLUMN salary_visibility VARCHAR(20) NOT NULL DEFAULT 'private'
    CHECK (salary_visibility IN ('private', 'enterprise_only', 'public'));

CREATE TABLE user_experiences (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    company VARCHAR(120) NOT NULL,
    title VARCHAR(120) NOT NULL,
    description TEXT,
    started_on DATE NOT NULL,
    ended_on DATE,  -- NULL = current
    position INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_user_experiences_user ON user_experiences (user_id, started_on DESC);

CREATE TABLE user_educations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    school VARCHAR(120) NOT NULL,
    degree VARCHAR(120),
    field VARCHAR(120),
    started_on DATE NOT NULL,
    ended_on DATE,
    position INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_user_educations_user ON user_educations (user_id, started_on DESC);

CREATE TABLE user_languages (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    language CHAR(2) NOT NULL,  -- ISO 639-1 code
    proficiency VARCHAR(10) NOT NULL CHECK (proficiency IN ('A1', 'A2', 'B1', 'B2', 'C1', 'C2', 'native')),
    PRIMARY KEY (user_id, language)
);
