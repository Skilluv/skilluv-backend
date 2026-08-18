-- What a public status page said about a window somebody claimed.
--
-- ## Why this is not an integration
--
-- The backlog asked for an automatic import from StatusPage, Instana or
-- Datadog. Two of those three need an API key to somebody's monitoring, and
-- such a key carries the service map, the incident history and the traffic
-- volumes of their estate — the list docs/ops/LEGAL.md says this platform
-- never holds and never brokers.
--
-- So the position, written down rather than left as a gap: **Skilluv reads
-- what is already public and nothing else.** A status page anybody can open
-- is read automatically; everything behind a credential stays declared,
-- sourced and read by a person.
--
-- ## Why it sits beside the figure rather than replacing it
--
-- A status page records the incidents its operator chose to publish. An
-- outage nobody posted is invisible here exactly as it is everywhere else,
-- so this cannot be the number. What it is: the public record, next to the
-- claim, with dates — enough for a reviewer to see when the two disagree.
--
-- No new URL column. The objective already names where its figure comes
-- from, and if that address happens to be a public status page, it is read.

ALTER TABLE ops_service_objectives
    ADD COLUMN public_observation JSONB,
    ADD COLUMN public_observed_at TIMESTAMPTZ,

    ADD CONSTRAINT public_observation_is_an_object CHECK (
        public_observation IS NULL OR jsonb_typeof(public_observation) = 'object'
    ),

    -- An observation with no date is one nobody can judge the freshness of,
    -- and a date with no observation claims a reading that never happened.
    ADD CONSTRAINT an_observation_says_when_it_was_taken CHECK (
        (public_observation IS NULL) = (public_observed_at IS NULL)
    );

COMMENT ON COLUMN ops_service_objectives.public_observation IS
    'What the evidence URL published, when it is a public status page: the '
    'incidents in the window and the availability they imply. Read without '
    'any credential, and never a replacement for the declared figure — a '
    'page shows the outages its operator chose to post.';

COMMENT ON COLUMN ops_service_objectives.public_observed_at IS
    'When the page was read. An observation with no date cannot be judged '
    'for freshness, and a stale one must not read as current.';
