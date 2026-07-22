-- Content strategy foundation — AI disclosure on deliverables (2026-07-21 decision).
--
-- Rationale: after debate, Skilluv chose NOT to categorize artifacts with AI
-- badges (ai-assisted / fully-manual / ai-primary). Rationale: any badge
-- creates perceived hierarchy even if the platform tries to stay neutral.
-- Users would judge ai-heavy artifacts as "less valuable" by social pressure.
--
-- What we keep from the charter §5.5:
--   - Disclosure required as free text in PR description when > 30% AI
--   - Sub-30% disclosure optional, not required
--   - Non-disclosure proven > 30% = charter Tier 2 violation
--   - Mentor review is the truth signal (they see the person defend the artifact)
--   - No automatic AI detection (unreliable, false positives)
--
-- This migration adds a single optional TEXT column on deliverables to store
-- the disclosure text extracted (or entered manually) from the PR description.
-- NULL = no disclosure declared. Non-NULL = user acknowledged some AI use.
--
-- Extraction heuristic (application-side): scan PR description for keywords
-- "AI", "IA", "Claude", "GPT", "Copilot", "Gemini", "Llama", "Cursor", if
-- found, prompt the user to formalize their disclosure before submission.

ALTER TABLE deliverables
    ADD COLUMN ai_disclosure TEXT;

COMMENT ON COLUMN deliverables.ai_disclosure IS
    'Free-text disclosure from the contributor about their AI usage on this deliverable. Extracted heuristically from PR description containing AI/IA/Claude/GPT/Copilot/Gemini/Llama/Cursor keywords, or entered explicitly. NULL = no AI disclosure declared. Non-obligatory below 30% AI, obligatory above (charter §5.5). Non-disclosure proven > 30% = Tier 2 violation (see charter §4).';
