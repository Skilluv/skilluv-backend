-- SKI-114 (P26 v2 H-02) — anti-fraud: a single PR URL cannot be attached
-- to two active challenges at once.
--
-- Without this, a challenger could:
--   1. Claim slice A on repo X, open PR #42, submit-pr → status='submitted'
--   2. Claim slice B on repo X (or Y!), submit the SAME PR #42 → also 'submitted'
--   3. Farm rewards from the same piece of work.
--
-- Partial unique index — scoped to "active" statuses so historical rows
-- (closed, expired) can accumulate freely without polluting the constraint.
-- `merged` is deliberately included: a merged slice's PR is the source of
-- truth for that outcome and cannot be re-attached.
CREATE UNIQUE INDEX IF NOT EXISTS uq_slices_submitted_pr_url_active
    ON project_slices (submitted_pr_url)
    WHERE submitted_pr_url IS NOT NULL
      AND status IN ('submitted','ci_green','pending_validation','validated','merged');
