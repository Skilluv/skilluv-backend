-- Two gaps the security marketplace could not open without.
--
-- ## 1. A signature for the NDA the mission already required
--
-- `missions.nda_required` has existed since 0192 and nothing has ever recorded
-- a signature. The flag said "you must sign" and the platform had nowhere to
-- put the fact that somebody did, so it was advice. Security is where that
-- stops being tolerable: most engagements are confidential before they are
-- signed, and a client whose scope becomes readable to every applicant does
-- not publish a second mission.
--
-- ### What this is and is not, legally
--
-- It is a simple electronic signature: the person is authenticated, they are
-- shown a document, they accept it, and what is recorded is the SHA-256 of the
-- exact bytes they were shown, the time, the address and the user agent. Under
-- eIDAS that is admissible and rebuttable — the lowest of the three tiers.
--
-- It is not an advanced or qualified signature. Ticket M-06 proposed
-- self-hosting DocuSeal, which would produce the same eIDAS tier through more
-- moving parts, and a commercial provider for the tier above. Neither is
-- refused for later; what is refused is pretending the difference does not
-- exist, and `docs/security/LEGAL.md` says which tier this is in the same
-- words.
--
-- The document hash is the load-bearing column. Without it a signature says
-- "somebody clicked yes" and the client can substitute the document
-- afterwards; with it, either party can prove what was actually agreed.
--
-- ## 2. `security_triager`
--
-- W-03 asked for a capability narrower than a reviewer: somebody who reads an
-- incoming finding, decides whether it is worth a reviewer's time, and either
-- passes it on or closes it. That is a different and much larger job than
-- reviewing — thirty reports a week, most of them not vulnerabilities — and
-- tying it to `security_reviewer:*` would mean either handing triage to people
-- who should be reviewing, or refusing it to people who are good at it and are
-- not senior enough to judge a finding on the merits.

-- ═══════════════════════════════════════════════════════════════════
-- Signatures
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE mission_nda_signatures (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mission_id UUID NOT NULL REFERENCES missions(id) ON DELETE CASCADE,
    signer_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,

    -- Which template or uploaded document, and the hash of the exact bytes the
    -- signer was shown. The hash is what makes this worth having: without it,
    -- a signature proves a click and not an agreement.
    document_url VARCHAR(500) NOT NULL,
    document_sha256 CHAR(64) NOT NULL,
    -- Which agreement: one of the platform's two, or `client_custom` for a
    -- document the client brought. Recorded rather than derived from the URL,
    -- because the URL will move and the terms that were agreed will not.
    template VARCHAR(20) NOT NULL
        CHECK (template IN ('mutual_standard', 'mutual_extended', 'client_custom')),

    signed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Evidence of the act. `INET` rather than text so that a range query is
    -- possible if a signature is ever contested.
    --
    -- Nullable, and that is deliberate: behind a reverse proxy that does not
    -- set a forwarded-for header — a development machine, a misconfigured
    -- deployment — there is no address the platform can honestly attribute.
    -- Recording `0.0.0.0` would put a false fact in a legal record, and an
    -- absent one is worth more than an invented one.
    signer_ip INET,
    signer_user_agent TEXT,
    -- What the signer typed as their name. Not verified against anything and
    -- kept anyway: it is part of what was done, and a dispute reads it.
    signer_typed_name VARCHAR(120) NOT NULL
        CHECK (length(btrim(signer_typed_name)) >= 2),

    -- An NDA can be released early by the client — a mission cancelled before
    -- it started, a scope that turned out to be public. Recorded rather than
    -- deleted: the obligation existed, and whether it still does is a
    -- different question from whether it ever did.
    released_at TIMESTAMPTZ,
    released_reason TEXT,

    -- One signature per person per mission. Re-signing a changed document
    -- means a changed mission, which is a new mission.
    UNIQUE (mission_id, signer_user_id),
    CONSTRAINT a_release_says_why CHECK (
        (released_at IS NULL) = (released_reason IS NULL)
    )
);

COMMENT ON TABLE mission_nda_signatures IS
    'Click-through acceptance of a confidentiality agreement, with the SHA-256 '
    'of the exact document shown. A simple electronic signature under eIDAS: '
    'admissible, rebuttable, and honest about which tier it is.';

COMMENT ON COLUMN mission_nda_signatures.document_sha256 IS
    'Hash of the bytes the signer was shown. The load-bearing column: without '
    'it a signature proves a click, and the document can be substituted after '
    'the fact.';

COMMENT ON COLUMN mission_nda_signatures.released_at IS
    'Set when the client releases the obligation early. Never a delete — that '
    'the obligation existed is a fact, separate from whether it still binds.';

CREATE INDEX idx_mission_nda_signatures_mission
    ON mission_nda_signatures (mission_id, signed_at DESC);
CREATE INDEX idx_mission_nda_signatures_signer
    ON mission_nda_signatures (signer_user_id, signed_at DESC);

-- ═══════════════════════════════════════════════════════════════════
-- Triage
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO capability_catalog (capability, family, scope, description, is_derived) VALUES
    ('security_triager', 'security_triager', NULL,
     'Reads incoming vulnerability reports and decides which reach a reviewer. '
     'Narrower than security_reviewer on purpose: triage is high-volume '
     'judgement about whether a report is worth somebody''s afternoon, not a '
     'judgement about whether the vulnerability is real.',
     FALSE)
ON CONFLICT (capability) DO NOTHING;
