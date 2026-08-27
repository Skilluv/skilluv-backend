-- The deliverable artifact type a confirmed mod carries.
--
-- Migration 0585 gave deliverables a game_mod_id parent and widened the
-- "at least one parent" constraint so a mod counts toward the cross-domain
-- rank. What it left is the artifact_type: the CHECK from 0236 has
-- playable_build, game_asset and game_scene, but a mod is none of those — it
-- is content hosted inside someone else's game, on a platform we do not own.
-- The services layer, creating that deliverable when a reviewer confirms a
-- mod, needs a type that says so.

ALTER TABLE deliverables DROP CONSTRAINT IF EXISTS deliverables_artifact_type_check;

ALTER TABLE deliverables
    ADD CONSTRAINT deliverables_artifact_type_check
    CHECK (artifact_type IN (
        'pr_merged',
        'pr_open',
        'commit',
        'design_artifact',
        'figma_frame',
        'design_tokens_export',
        'playable_build',
        'game_asset',
        'game_scene',
        'game_mod',               -- a mod, hosted on the platform its game uses
        'cve_report',
        'pentest_writeup',
        'disclosure',
        'code_review',
        'documentation',
        'test_suite',
        'blender_asset',
        'other'
    ));
