-- Phase P1 — Slices deviennent le modèle unifié.
-- Migration 0063 : backfill des `oss_bounties` existantes comme `project_slices`.
--
-- Pour chaque bounty non-lié à une slice :
--   1. On cherche le projet Skilluv correspondant (match sur github_repo_owner/name)
--   2. Si trouvé → INSERT project_slice + UPDATE oss_bounty.slice_id
--   3. Si pas trouvé → skip (RAISE NOTICE pour tracer)
--
-- Idempotence : la clause `WHERE ob.slice_id IS NULL` garantit qu'on ne
-- re-traite pas les bounties déjà migrées. La migration peut être re-jouée
-- sans doublon (ex: après ajout d'un nouveau projet qui matche des bounties
-- précédemment orphelines).
--
-- Statuts mappés :
--   bounty.open       → slice.open
--   bounty.claimed    → slice.claimed (mais claimed_by est None ici, il vient
--                       du oss_bounty_claims lié ; on laisse le service applicatif
--                       du Phase P1 reconstruire la relation dans un second temps)
--   bounty.in_review  → slice.in_review
--   bounty.paid       → slice.merged
--   bounty.cancelled  → slice.closed
--   bounty.expired    → slice.expired

DO $$
DECLARE
    bounty_row RECORD;
    matched_project_id UUID;
    new_slice_id UUID;
    mapped_status VARCHAR(20);
    migrated_count INTEGER := 0;
    skipped_count INTEGER := 0;
BEGIN
    FOR bounty_row IN
        SELECT ob.id, ob.repo_owner, ob.repo_name, ob.issue_number, ob.issue_url,
               ob.title, ob.description, ob.reward_credits, ob.fragments_bonus,
               ob.difficulty, ob.tags, ob.status, ob.expires_at, ob.created_at,
               ob.posted_by_user_id
        FROM oss_bounties ob
        WHERE ob.slice_id IS NULL
    LOOP
        -- Matching projet par owner/name GitHub
        SELECT p.id INTO matched_project_id
        FROM projects p
        WHERE p.github_repo_owner = bounty_row.repo_owner
          AND p.github_repo_name = bounty_row.repo_name
        LIMIT 1;

        IF matched_project_id IS NULL THEN
            skipped_count := skipped_count + 1;
            RAISE NOTICE 'Bounty % skipped (no matching project for %/%)',
                bounty_row.id, bounty_row.repo_owner, bounty_row.repo_name;
            CONTINUE;
        END IF;

        -- Mapping status
        mapped_status := CASE bounty_row.status
            WHEN 'open'      THEN 'open'
            WHEN 'claimed'   THEN 'claimed'
            WHEN 'in_review' THEN 'in_review'
            WHEN 'paid'      THEN 'merged'
            WHEN 'cancelled' THEN 'closed'
            WHEN 'expired'   THEN 'expired'
            ELSE 'closed'
        END;

        -- Création de la slice
        INSERT INTO project_slices (
            project_id, slice_type, external_ref, external_metadata,
            title, description, acceptance_criteria,
            primary_domain, difficulty, fragments_reward, credits_reward,
            status, claim_expires_at,
            created_by_user_id, ingested_from,
            created_at, updated_at
        ) VALUES (
            matched_project_id,
            'github_issue',
            bounty_row.issue_number::TEXT,
            jsonb_build_object(
                'source', 'legacy_bounty_backfill',
                'issue_url', bounty_row.issue_url,
                'tags', bounty_row.tags,
                'legacy_bounty_id', bounty_row.id
            ),
            bounty_row.title,
            bounty_row.description,
            NULL,
            'code',                                    -- best-effort, adjustable par steward
            bounty_row.difficulty::SMALLINT,
            bounty_row.fragments_bonus,
            bounty_row.reward_credits,
            mapped_status,
            bounty_row.expires_at,
            bounty_row.posted_by_user_id,
            'legacy_bounty',
            bounty_row.created_at,
            NOW()
        )
        RETURNING id INTO new_slice_id;

        -- Lier la bounty à sa nouvelle slice
        UPDATE oss_bounties
        SET slice_id = new_slice_id,
            updated_at = NOW()
        WHERE id = bounty_row.id;

        migrated_count := migrated_count + 1;
    END LOOP;

    RAISE NOTICE 'Bounties backfill: % migrated, % skipped (no matching project)',
        migrated_count, skipped_count;
END $$;
