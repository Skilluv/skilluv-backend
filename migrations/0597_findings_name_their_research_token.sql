-- Which research token a finding arrived under.
--
-- ## The question the admin surface could not answer
--
-- `POST /api/admin/security/research-tokens/{id}/revoke` has existed since
-- 0548 and nothing lists what there is to revoke, so the only way to reach it
-- was with an id out of psql. The listing that fixes that has to be worth
-- opening, and the number that makes it worth opening is "what has this token
-- actually produced" — a token behind four confirmed findings and a token
-- behind nothing but traffic are not the same decision.
--
-- That number was not derivable. Findings recorded who reported them and
-- tokens recorded how many requests they had seen, and nothing joined the two.
-- Counting a holder's findings by date range instead would have been a guess
-- dressed as a figure: a researcher may report through the form with no token
-- set at all, and the guess would credit the token for it.
--
-- ## Why it is nullable, and stays nullable
--
-- Most findings arrive without one. A token raises a rate limit and grants
-- nothing else (0548 is emphatic about this), so requiring one to report would
-- turn a convenience into a gate on the disclosure programme. NULL means "sent
-- without a token", which is the ordinary case and not a defect.
--
-- ## Why SET NULL rather than CASCADE
--
-- Revoking a token must never delete a finding, and neither must pruning one.
-- The finding is the durable object here; the token is the credential it came
-- in under. If the token row ever goes, the finding stays and simply stops
-- naming it.

ALTER TABLE security_findings
    ADD COLUMN research_token_id UUID
        REFERENCES security_research_tokens(id) ON DELETE SET NULL;

COMMENT ON COLUMN security_findings.research_token_id IS
    'The research token in force when this report was submitted, or NULL when '
    'it was sent without one. Set from the request scope, never from the body: '
    'a client could otherwise credit somebody else''s token.';

-- Counting findings per token is the whole reason the column exists.
CREATE INDEX idx_security_findings_research_token
    ON security_findings (research_token_id)
    WHERE research_token_id IS NOT NULL;
