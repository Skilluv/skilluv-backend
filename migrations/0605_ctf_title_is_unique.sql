-- Two capture-the-flag challenges may not share a title.
--
-- ## Why now
--
-- The Juice Shop catalogue is derived rather than written (SKI-137): the flag
-- for each challenge comes from the instance's `ctfKey` and the challenge's
-- name, so the twenty rows are recomputed whenever that key changes. Migration
-- 0602's Discord step established the shape — a seed step whose ledger version
-- is the configuration's fingerprint, so a rotated key re-runs it.
--
-- Recomputing needs somewhere to land. Without a uniqueness key the second run
-- inserts twenty more rows, and a person browsing the catalogue sees each
-- challenge twice with only one of the pair carrying a flag that still works.
-- The title is what identifies a challenge to a reader, so it is what the
-- upsert keys on.
--
-- ## Why only the flag challenges
--
-- Scoped to `security_kind = 'ctf_flag'` rather than applied to the whole
-- table. Every other challenge in this catalogue is written by a person for
-- one orientation, and two trades may legitimately want a challenge called
-- "Refactor the payment flow". A capture-the-flag challenge is a puzzle with
-- one answer; two of them under one name is a mistake in every case.
--
-- Verified empty before adding: there are no `ctf_flag` rows yet, so nothing
-- has to be reconciled.

CREATE UNIQUE INDEX challenge_templates_ctf_title_unique
    ON challenge_templates (title)
    WHERE security_kind = 'ctf_flag';

COMMENT ON INDEX challenge_templates_ctf_title_unique IS
    'The upsert target for the derived Juice Shop catalogue (SKI-137). A '
    'rotated ctfKey re-derives all twenty flags; without this they would be '
    'inserted a second time and half the catalogue would silently stop '
    'accepting correct answers.';
