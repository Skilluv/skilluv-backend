-- The forge with the door open.
--
-- ## What this replaces
--
-- The landing page carries a ticker labelled LIVE with eight invented people
-- on it. Skilluv's whole position is "nobody knows what people can do, and we
-- prove it"; fabricated social proof is the exact failure the product exists
-- to correct, and it is the one claim on that page a careful visitor can
-- check.
--
-- ## Why a projection and not a query
--
-- The obvious implementation joins deliverables, attestations and payouts at
-- request time and filters for the public ones. A landing page is the most
-- exposed surface the product has, and that shape leaks the first time
-- somebody adds a join and forgets a predicate — silently, to everybody.
--
-- So: an explicit table, written by the hooks that already fire, with a
-- `public` flag decided at write time. Nothing reaches this table by
-- accident, and reading it cannot expose a column that is not on it.
--
-- ## What is admitted
--
-- Only events backed by an artefact somebody outside Skilluv can go and look
-- at: a merged pull request, a verified deliverable, an issued attestation, a
-- bounty actually paid. Never a self-declared event, never a points counter.
-- A feed of points proves nothing to anybody, which is what the ticker it
-- replaces was.

CREATE TABLE public_artifact_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- What happened. Each of these has an artefact behind it; nothing else is
    -- admitted, and the CHECK is the enforcement rather than a convention.
    kind VARCHAR(40) NOT NULL CHECK (kind IN (
        'pr_merged_upstream',
        'deliverable_verified',
        'attestation_issued',
        'bounty_paid',
        'mission_delivered',
        'library_published'
    )),

    -- Who it is about. A guild for team work, so a collective delivery reads
    -- as one line rather than five.
    subject_type VARCHAR(20) NOT NULL CHECK (subject_type IN ('user', 'guild')),
    subject_id UUID NOT NULL,
    -- Denormalised at write time. The point of a projection is that reading
    -- it touches nothing private, and a join to `users` for a display name
    -- would put that back.
    subject_label VARCHAR(120) NOT NULL,

    -- What to say. Written by the emitter, in the platform's own words, not
    -- assembled from fragments at render time — a sentence built in three
    -- places is a sentence nobody can translate.
    headline TEXT NOT NULL CHECK (btrim(headline) <> ''),
    -- Where the artefact is. Either a Skilluv verification page or the
    -- upstream URL. An event with nowhere to go is a claim, which is what
    -- this table exists to stop.
    artifact_url TEXT NOT NULL CHECK (artifact_url ~ '^https?://'),
    -- The upstream repository, when there is one. Shown because "merged on
    -- calcom/cal.com" carries more than "merged a pull request".
    repository VARCHAR(200),

    -- The money, when the event is about money. Amount and currency together
    -- or neither: a figure with no currency is not a figure.
    amount NUMERIC(14,2),
    currency CHAR(3),

    -- Decided at write time from the subject's preference and the kind's
    -- default. Never computed at read time: that is the join this table
    -- exists to avoid.
    public BOOLEAN NOT NULL,
    -- Why it is or is not public, for whoever has to answer the question
    -- later. 'consented', 'default_public', 'opted_out', 'kind_private'.
    visibility_reason VARCHAR(30) NOT NULL,

    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- What this event is a projection of. Lets a revocation find its line
    -- and take it down.
    source_type VARCHAR(40) NOT NULL,
    source_id UUID NOT NULL,

    -- One line per source event. A deliverable verified twice by two code
    -- paths is one thing that happened.
    UNIQUE (source_type, source_id, kind),

    CONSTRAINT money_carries_its_currency CHECK (
        (amount IS NULL) = (currency IS NULL)
    ),
    CONSTRAINT money_is_positive CHECK (amount IS NULL OR amount > 0)
);

COMMENT ON TABLE public_artifact_events IS
    'The public feed, as a projection written by the hooks. Not a query over '
    'private tables: a landing page is the most exposed surface there is, and '
    'a missing predicate on a join leaks to everybody at once.';

COMMENT ON COLUMN public_artifact_events.public IS
    'Decided at write time from the subject''s preference. Computing it at '
    'read time would be the join this table exists to avoid.';

-- The feed reads exactly this: public, newest first, keyset paginated.
CREATE INDEX idx_public_artifact_feed
    ON public_artifact_events (occurred_at DESC, id DESC)
    WHERE public = TRUE;

CREATE INDEX idx_public_artifact_subject
    ON public_artifact_events (subject_type, subject_id, occurred_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Consent
-- ═══════════════════════════════════════════════════════════════════
--
-- Showing somebody's name and what they did on a public page is a
-- publication. `users.profile_hidden` does not cover it: somebody can
-- reasonably want a public profile and not want a ticker announcing each
-- thing they do.
--
-- ## The defaults, and why they differ
--
-- A merged pull request is already public on GitHub — announcing it repeats
-- something anybody can already read, so it defaults to visible. A payment is
-- not public anywhere, and defaulting it to visible would publish somebody's
-- income because they took a bounty. That one is off unless asked for.
--
-- This is the same reasoning the notification catalogue uses for push, and
-- deliberately the same shape.

CREATE TABLE public_feed_event_kinds (
    kind VARCHAR(40) PRIMARY KEY,
    -- What the person sees when choosing. Written for them, not for us.
    label VARCHAR(160) NOT NULL,
    description TEXT NOT NULL,
    -- Whether the underlying artefact is already public elsewhere. The whole
    -- basis for the default below.
    already_public_elsewhere BOOLEAN NOT NULL,
    default_visible BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- An event that is not public elsewhere cannot default to visible. Not a
    -- style rule: it is the difference between repeating something and
    -- publishing it.
    CONSTRAINT private_events_do_not_default_visible CHECK (
        already_public_elsewhere OR NOT default_visible
    )
);

INSERT INTO public_feed_event_kinds
    (kind, label, description, already_public_elsewhere, default_visible)
VALUES
    ('pr_merged_upstream',
     'Mes contributions fusionnées',
     'Une pull request fusionnée dans un projet tiers. Déjà publique sur le dépôt : le flux ne fait que la relayer.',
     TRUE, TRUE),

    ('deliverable_verified',
     'Mes livrables validés',
     'Un artefact vérifié par Skilluv, avec son lien de vérification publique.',
     TRUE, TRUE),

    ('attestation_issued',
     'Mes attestations',
     'Une attestation émise, vérifiable par son code public.',
     TRUE, TRUE),

    ('library_published',
     'Mes bibliothèques publiées',
     'Un paquet publié sur un registre. Déjà public : n''importe qui peut l''installer.',
     TRUE, TRUE),

    ('mission_delivered',
     'Mes missions livrées',
     'Une mission payée menée à son terme. Le montant n''est jamais affiché.',
     FALSE, FALSE),

    ('bounty_paid',
     'Mes primes reçues',
     'Une prime versée, avec son montant. Désactivé par défaut : ce que tu gagnes ne regarde que toi.',
     FALSE, FALSE);

COMMENT ON TABLE public_feed_event_kinds IS
    'What can appear in the public feed, and whether it defaults to visible. '
    'A merged pull request is already public and repeating it is fair; a '
    'payment is not, and defaulting it visible would publish somebody''s '
    'income because they took a bounty.';

CREATE TABLE public_feed_preferences (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind VARCHAR(40) NOT NULL REFERENCES public_feed_event_kinds(kind) ON DELETE CASCADE,
    visible BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, kind)
);

COMMENT ON TABLE public_feed_preferences IS
    'Per person, per kind. Absent means the kind''s default applies — rows '
    'are written only when somebody actually chose, so a change of default '
    'reaches the people who never expressed an opinion and nobody else.';

-- ═══════════════════════════════════════════════════════════════════
-- Deciding visibility, in one place
-- ═══════════════════════════════════════════════════════════════════
--
-- Called by the emitter, at write time. A function rather than three lines
-- repeated in six hooks, one of which would eventually read the default
-- backwards.

CREATE OR REPLACE FUNCTION public_feed_visibility(
    subject_user UUID,
    event_kind TEXT
) RETURNS TABLE (visible BOOLEAN, reason TEXT) AS $$
DECLARE
    chosen BOOLEAN;
    kind_default BOOLEAN;
    hidden_profile BOOLEAN;
    banned BOOLEAN;
BEGIN
    SELECT default_visible INTO kind_default
      FROM public_feed_event_kinds WHERE kind = event_kind;

    IF kind_default IS NULL THEN
        RETURN QUERY SELECT FALSE, 'kind_private'::TEXT;
        RETURN;
    END IF;

    SELECT u.profile_hidden, u.is_banned INTO hidden_profile, banned
      FROM users u WHERE u.id = subject_user;

    -- Somebody who hid their profile is not somebody who wants a ticker
    -- announcing them, whatever the per-kind preference says.
    IF COALESCE(hidden_profile, TRUE) OR COALESCE(banned, TRUE) THEN
        RETURN QUERY SELECT FALSE, 'opted_out'::TEXT;
        RETURN;
    END IF;

    SELECT p.visible INTO chosen
      FROM public_feed_preferences p
     WHERE p.user_id = subject_user AND p.kind = event_kind;

    IF chosen IS NOT NULL THEN
        RETURN QUERY SELECT chosen, CASE WHEN chosen THEN 'consented' ELSE 'opted_out' END::TEXT;
        RETURN;
    END IF;

    RETURN QUERY SELECT kind_default,
                        CASE WHEN kind_default THEN 'default_public' ELSE 'kind_private' END::TEXT;
END;
$$ LANGUAGE plpgsql STABLE;

-- ═══════════════════════════════════════════════════════════════════
-- Taking a line down
-- ═══════════════════════════════════════════════════════════════════
--
-- A revoked deliverable must leave the feed. Not deleted: the row is the
-- record that it was shown, and somebody investigating a complaint needs to
-- see that it was, and when it stopped being.

ALTER TABLE public_artifact_events
    ADD COLUMN retracted_at TIMESTAMPTZ,
    ADD COLUMN retraction_reason TEXT,
    ADD CONSTRAINT retraction_carries_a_reason CHECK (
        retracted_at IS NULL
        OR (retraction_reason IS NOT NULL AND btrim(retraction_reason) <> '')
    );

CREATE OR REPLACE FUNCTION retract_public_events_for_source(
    source TEXT,
    source_key UUID,
    why TEXT
) RETURNS INTEGER AS $$
DECLARE
    affected INTEGER;
BEGIN
    UPDATE public_artifact_events
       SET public = FALSE,
           retracted_at = NOW(),
           retraction_reason = why
     WHERE source_type = source
       AND source_id = source_key
       AND retracted_at IS NULL;
    GET DIAGNOSTICS affected = ROW_COUNT;
    RETURN affected;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION retract_public_events_for_source(TEXT, UUID, TEXT) IS
    'Take a line down without deleting it: the row is the record that it was '
    'shown, and somebody investigating a complaint needs to see when it '
    'stopped being.';

-- A revoked deliverable takes its line down by itself. The alternative is
-- remembering to call the retraction from every path that revokes, and one
-- of them would forget.
CREATE OR REPLACE FUNCTION revoked_deliverable_leaves_the_feed()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.revoked_at IS NOT NULL AND OLD.revoked_at IS NULL THEN
        PERFORM retract_public_events_for_source(
            'deliverable', NEW.id, 'le livrable a été révoqué'
        );
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_revoked_deliverable_leaves_the_feed
    AFTER UPDATE ON deliverables
    FOR EACH ROW EXECUTE FUNCTION revoked_deliverable_leaves_the_feed();

CREATE OR REPLACE FUNCTION revoked_attestation_leaves_the_feed()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.revoked_at IS NOT NULL AND OLD.revoked_at IS NULL THEN
        PERFORM retract_public_events_for_source(
            'attestation', NEW.id, 'l''attestation a été révoquée'
        );
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_revoked_attestation_leaves_the_feed
    AFTER UPDATE ON attestations
    FOR EACH ROW EXECUTE FUNCTION revoked_attestation_leaves_the_feed();

-- ═══════════════════════════════════════════════════════════════════
-- Getting on the feed
-- ═══════════════════════════════════════════════════════════════════
--
-- Two of the six kinds are written from many places: a deliverable reaches
-- `verified` through a webhook, a review, an admin action and a poller, and
-- an attestation is issued from four services. Emitting from the service
-- layer would mean finding all of them and remembering the next one.
--
-- So those two are triggers. The other four — a published library, a bounty
-- paid, a mission delivered — each have exactly one writer, in code that
-- already knows the wording, and are emitted from there.
--
-- The wording lives here rather than being assembled at render time: a
-- sentence built in three places is a sentence nobody can translate.

CREATE OR REPLACE FUNCTION verified_deliverable_reaches_the_feed()
RETURNS TRIGGER AS $$
DECLARE
    author RECORD;
    decision RECORD;
    what TEXT;
    repo TEXT;
    event_kind TEXT;
    link TEXT;
BEGIN
    IF NEW.verification_status <> 'verified'
       OR OLD.verification_status IS NOT DISTINCT FROM 'verified'
       OR NEW.revoked_at IS NOT NULL THEN
        RETURN NEW;
    END IF;

    SELECT u.id, u.username INTO author FROM users u WHERE u.id = NEW.user_id;
    IF author.id IS NULL THEN
        RETURN NEW;
    END IF;

    -- A merged pull request is its own kind: it is the one that reads as
    -- "contributed to somebody else's project", which is the whole argument.
    event_kind := CASE WHEN NEW.artifact_type = 'pr_merged'
                       THEN 'pr_merged_upstream'
                       ELSE 'deliverable_verified' END;

    SELECT COALESCE(s.title, ct.title),
           CASE WHEN p.github_repo_owner IS NOT NULL
                THEN p.github_repo_owner || '/' || p.github_repo_name END
      INTO what, repo
      FROM deliverables d
      LEFT JOIN project_slices s ON s.id = d.slice_id
      LEFT JOIN projects p ON p.id = s.project_id
      LEFT JOIN challenge_templates ct ON ct.id = d.challenge_id
     WHERE d.id = NEW.id;

    link := NEW.artifact_url;
    IF link IS NULL OR link !~ '^https?://' THEN
        -- An event with nowhere to go is a claim. Skipped rather than shown.
        RETURN NEW;
    END IF;

    SELECT * INTO decision FROM public_feed_visibility(NEW.user_id, event_kind);

    INSERT INTO public_artifact_events
        (kind, subject_type, subject_id, subject_label, headline,
         artifact_url, repository, public, visibility_reason,
         source_type, source_id, occurred_at)
    VALUES (
        event_kind, 'user', author.id, author.username,
        CASE WHEN event_kind = 'pr_merged_upstream' AND repo IS NOT NULL
             THEN format('contribution fusionnée sur %s', repo)
             WHEN event_kind = 'pr_merged_upstream'
             THEN 'contribution fusionnée en amont'
             ELSE format('livrable validé : %s', COALESCE(what, 'un artefact'))
        END,
        link, repo, decision.visible, decision.reason,
        'deliverable', NEW.id, COALESCE(NEW.verified_at, NOW())
    )
    ON CONFLICT (source_type, source_id, kind) DO NOTHING;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_verified_deliverable_reaches_the_feed
    AFTER UPDATE ON deliverables
    FOR EACH ROW EXECUTE FUNCTION verified_deliverable_reaches_the_feed();

CREATE OR REPLACE FUNCTION issued_attestation_reaches_the_feed()
RETURNS TRIGGER AS $$
DECLARE
    holder RECORD;
    decision RECORD;
BEGIN
    IF NOT NEW.public OR NEW.revoked_at IS NOT NULL THEN
        RETURN NEW;
    END IF;

    SELECT u.id, u.username INTO holder FROM users u WHERE u.id = NEW.user_id;
    IF holder.id IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT * INTO decision FROM public_feed_visibility(NEW.user_id, 'attestation_issued');

    INSERT INTO public_artifact_events
        (kind, subject_type, subject_id, subject_label, headline,
         artifact_url, public, visibility_reason, source_type, source_id,
         occurred_at)
    VALUES (
        'attestation_issued', 'user', holder.id, holder.username,
        format('attestation émise, vérifiable : %s', NEW.title),
        -- The verification page, which is the point: every line on this feed
        -- leads somewhere a stranger can check.
        'https://skill-uv.com/verify/' || NEW.verification_code,
        decision.visible, decision.reason,
        'attestation', NEW.id, NEW.issued_at
    )
    ON CONFLICT (source_type, source_id, kind) DO NOTHING;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_issued_attestation_reaches_the_feed
    AFTER INSERT ON attestations
    FOR EACH ROW EXECUTE FUNCTION issued_attestation_reaches_the_feed();
