-- Challenges table
CREATE TABLE challenges (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR(200) NOT NULL,
    description TEXT NOT NULL,
    instructions TEXT NOT NULL,
    skill_domain VARCHAR(20) NOT NULL CHECK (skill_domain IN ('code', 'design', 'game', 'security')),
    difficulty SMALLINT NOT NULL CHECK (difficulty BETWEEN 1 AND 5),
    mode VARCHAR(10) NOT NULL DEFAULT 'solo' CHECK (mode IN ('solo', 'team')),
    duration_minutes INTEGER,
    ai_allowed BOOLEAN NOT NULL DEFAULT FALSE,
    tone VARCHAR(20) NOT NULL DEFAULT 'serious' CHECK (tone IN ('serious', 'fun', 'educational')),
    language VARCHAR(30),
    prerequisite_fragments INTEGER NOT NULL DEFAULT 0,
    reward_fragments INTEGER NOT NULL DEFAULT 10,
    is_onboarding BOOLEAN NOT NULL DEFAULT FALSE,
    status VARCHAR(20) NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published', 'archived')),
    -- Test configuration
    test_cases JSONB,
    expected_output TEXT,
    -- Metadata
    created_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_challenges_domain ON challenges (skill_domain);
CREATE INDEX idx_challenges_status ON challenges (status);
CREATE INDEX idx_challenges_onboarding ON challenges (is_onboarding, skill_domain) WHERE is_onboarding = TRUE;
CREATE INDEX idx_challenges_difficulty ON challenges (difficulty);

-- Challenge submissions
CREATE TABLE challenge_submissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    challenge_id UUID NOT NULL REFERENCES challenges(id),
    user_id UUID NOT NULL REFERENCES users(id),
    status VARCHAR(20) NOT NULL DEFAULT 'in_progress' CHECK (status IN ('in_progress', 'submitted', 'success', 'failure')),
    code TEXT,
    language VARCHAR(30),
    stdout TEXT,
    stderr TEXT,
    fragments_earned INTEGER NOT NULL DEFAULT 0,
    attempt_number INTEGER NOT NULL DEFAULT 1,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    submitted_at TIMESTAMPTZ,
    evaluated_at TIMESTAMPTZ
);

CREATE INDEX idx_submissions_user ON challenge_submissions (user_id);
CREATE INDEX idx_submissions_challenge ON challenge_submissions (challenge_id);
CREATE INDEX idx_submissions_user_challenge ON challenge_submissions (user_id, challenge_id);

-- Skill fragments per sub-skill
CREATE TABLE skill_fragments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id),
    skill_domain VARCHAR(20) NOT NULL,
    sub_skill VARCHAR(50) NOT NULL,
    fragments INTEGER NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, skill_domain, sub_skill)
);

CREATE INDEX idx_skill_fragments_user ON skill_fragments (user_id);
CREATE INDEX idx_skill_fragments_domain ON skill_fragments (user_id, skill_domain);

-- Seed onboarding challenges (one per domain)
INSERT INTO challenges (title, description, instructions, skill_domain, difficulty, duration_minutes, reward_fragments, is_onboarding, status, tone) VALUES
(
    'Premier pas — Hello World',
    'Bienvenue sur Skilluv ! Écris ton premier programme.',
    E'Écris un programme qui affiche exactement :\nHello, Skilluv!\n\nC''est tout. Simple, efficace. Ton premier fragment t''attend.',
    'code', 1, 10, 10, TRUE, 'published', 'educational'
),
(
    'Premier pas — Ton premier design',
    'Bienvenue sur Skilluv ! Crée ton premier visuel.',
    E'Décris en détail un logo pour une app de musique africaine.\nInclus : couleurs, forme, typographie, inspiration.\nMinimum 100 mots.',
    'design', 1, 10, 10, TRUE, 'published', 'educational'
),
(
    'Premier pas — Game Concept',
    'Bienvenue sur Skilluv ! Imagine ton premier jeu.',
    E'Décris un concept de jeu vidéo en 3 parties :\n1. Le pitch (2 phrases)\n2. La mécanique principale\n3. Ce qui le rend unique\nMinimum 100 mots.',
    'game', 1, 10, 10, TRUE, 'published', 'educational'
),
(
    'Premier pas — Trouve la faille',
    'Bienvenue sur Skilluv ! Repère ta première vulnérabilité.',
    E'Analyse ce code Python et identifie la faille de sécurité :\n\nimport sqlite3\ndef login(username, password):\n    conn = sqlite3.connect("users.db")\n    query = f"SELECT * FROM users WHERE username=''{username}'' AND password=''{password}''""\n    return conn.execute(query).fetchone()\n\nExplique : 1) Quelle est la faille ? 2) Comment l''exploiter ? 3) Comment la corriger ?',
    'security', 1, 10, 10, TRUE, 'published', 'educational'
);
