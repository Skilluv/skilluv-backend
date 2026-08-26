-- Paid security work, on the mission table that already exists.
--
-- ## Why there is no `cyber_missions`
--
-- Ticket M-01 asked for one, with a status machine, an applications table, a
-- deliveries table, a ratings table, a disputes table and an invoices table —
-- six tables. Migration 0301 refused that for AI, 0413 for audio, 0509 for
-- communication and 0526 for education, and the reason has not changed: all
-- six exist, keyed by `skill_domain`, with the endpoints and the arbitration
-- already written. A second set means a second answer to "how many missions
-- has this person delivered", and both get quoted.
--
-- What was actually missing, checked column by column against M-01:
--
--   * `mission_type` — eight values requested; `mission_types` is a table
--     since 0192 and eleven security rows are added below.
--   * `deliverables_md` — `acceptance_criteria`, and the shape of what is
--     handed over is `mission_deliverable_formats`, six security rows below.
--   * `required_orientations` (array) — `orientation_id`, singular, and it
--     stays singular. A mission that wants a red teamer *or* a governance
--     specialist is two missions: the scope, the report and the price are all
--     different, and an array here would let a client write one advert and
--     then reject half the applicants for not being the other thing.
--   * `min_rank` — genuinely absent. `project_slices` has had it since 0058
--     and `missions` never did, so a mission could not say "not for a first
--     week". Added below, same five values.
--   * `required_certs` — added as `required_credentials`, matched against
--     `external_credentials`, which is where a declared OSCP already lives.
--   * `scope_confidential` — added as `client_anonymous`, which is what it
--     actually controls: whether the listing names the client.
--   * `nda_required`, `ip_terms`, `payment_model`, `budget_eur`,
--     `apply_deadline`, `commission`, the status machine, the selected talent
--     — all already there.
--
-- ## The column this domain adds that no other needed
--
-- `rules_of_engagement_url`, and it is required for the offensive mission
-- types. 0550 made the same argument for slices: a security engagement without
-- a written authorisation is not a job, and the difference between a platform
-- that says so and one that enforces it is a constraint that refuses.
--
-- ## Disclosure rights (M-13)
--
-- Two booleans rather than four new `ip_terms` values. What M-13 was really
-- asking is not "who owns the report" — `ip_terms` answers that — but "may the
-- researcher ever talk about this", which is orthogonal: a client can own the
-- report outright and still permit a redacted write-up after the patch, and
-- that combination is common. Folding it into the ownership enum would have
-- made those two facts inexpressible together.
--
-- One `ip_terms` value is added, because it was a genuinely missing deal
-- shape: `licence_to_client`, where the researcher keeps the report and the
-- client gets a right to use it. M-13 called it `researcher_retains_all`, and
-- it is rare and real — usually with a reduced fee.

-- ═══════════════════════════════════════════════════════════════════
-- What a security mission can be
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order) VALUES
('sec_pentest_web', 'security', 'Web and API intrusion test',
 'A scoped test of an application and its interfaces, against a written rules-of-engagement document, with a report a developer can act on.', 610),
('sec_pentest_mobile', 'security', 'Mobile intrusion test',
 'The application, its storage, its transport and its backend, on a real device or an emulator.', 615),
('sec_pentest_infra', 'security', 'Infrastructure intrusion test',
 'Network, cloud configuration and identity. Judged on the path found, not on the port list.', 620),
('sec_code_audit', 'security', 'Security code audit',
 'Reading a codebase for exploitable defects: paths traced, reachability established, fixes proposed, and what the scanner got wrong.', 625),
('sec_threat_model', 'security', 'Threat model',
 'An architecture, before or alongside the code. Output is a prioritised list somebody can plan against, not a diagram.', 630),
('sec_compliance_audit', 'security', 'Compliance audit',
 'Controls read against a named framework — GDPR, ISO 27001, SOC 2 — with the gap list and the evidence an auditor would accept.', 635),
('sec_policy_authoring', 'security', 'Policy and governance authoring',
 'Writing the policies, procedures and registers an organisation is going to be audited on, so that they match what it does.', 640),
('sec_incident_response', 'security', 'Incident response',
 'Called in during or after an event: containment, evidence, timeline, and the notification duty.', 645),
('sec_detection_engineering', 'security', 'Detection engineering',
 'Rules built, tuned and validated by re-running the technique they were written for.', 650),
('sec_purple_exercise', 'security', 'Purple exercise',
 'Facilitating attack and defence in one exercise, with a detection as the deliverable.', 655),
('sec_bounty_programme_setup', 'security', 'Bug bounty programme setup',
 'Standing up a disclosure programme: scope, safe harbour, triage workflow, severity scale and the first months of triage.', 660);

INSERT INTO mission_deliverable_formats (slug, skill_domain, name, description, sort_order) VALUES
('sec_pentest_report', 'security', 'Intrusion test report',
 'Executive summary, method, findings each with severity, reproduction and remediation, and an appendix a technical reader can verify.', 610),
('sec_audit_report', 'security', 'Code audit report',
 'Findings with file and line, the traced path, the proposed fix, and the dismissed scanner hits with reasons.', 620),
('sec_threat_model_document', 'security', 'Threat model document',
 'The system as modelled, the threats named in a shared taxonomy, the ranking, and the mitigations with owners.', 630),
('sec_compliance_gap_analysis', 'security', 'Gap analysis',
 'Control by control against the framework: met, partially met, not met, with the evidence or the absence of it.', 640),
('sec_detection_ruleset', 'security', 'Detection ruleset',
 'Rules in a stated format, with the tests that fire them and the traffic they were shown to stay quiet on.', 650),
('sec_incident_report', 'security', 'Incident report',
 'Timeline, scope of compromise, indicators, what was done, and what would have caught it earlier.', 660);

-- ═══════════════════════════════════════════════════════════════════
-- The six columns
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE missions
    -- The gate `project_slices` has had since 0058 and this table never did.
    ADD COLUMN min_rank VARCHAR(20)
        CHECK (min_rank IS NULL OR min_rank IN
               ('apprenti', 'ranger', 'artisan', 'maitre', 'doyen')),
    -- Credentials an applicant has to have declared, matched against
    -- `external_credentials.name`. Declared is not verified, and the
    -- application response says which of the two it found — a client asking
    -- for an OSCP is entitled to know that nobody has checked it.
    ADD COLUMN required_credentials TEXT[] NOT NULL DEFAULT '{}',
    -- Whether the listing names the client. Most security engagements are
    -- confidential before they are signed, and a client who cannot advertise
    -- anonymously does not advertise.
    ADD COLUMN client_anonymous BOOLEAN NOT NULL DEFAULT FALSE,
    -- The written authorisation. Required for the offensive types, by the
    -- trigger below.
    ADD COLUMN rules_of_engagement_url VARCHAR(500)
        CHECK (rules_of_engagement_url IS NULL
               OR rules_of_engagement_url ~ '^(https://|/)'),
    -- May the person who did the work ever describe it. Orthogonal to
    -- ownership: a client can own the report outright and still permit a
    -- redacted write-up once the fix has shipped.
    ADD COLUMN allows_public_disclosure BOOLEAN NOT NULL DEFAULT FALSE,
    -- And if so, is the researcher named. Default true because being credited
    -- is most of why anybody writes the write-up.
    ADD COLUMN credits_researcher_in_disclosure BOOLEAN NOT NULL DEFAULT TRUE;

-- A security engagement that touches a live system says what it is allowed to
-- touch. Enforced, not recommended.
--
-- A trigger rather than a CHECK because the condition has to read
-- `mission_types` to know whether this mission is an offensive one, and a
-- CHECK may not contain a subquery. It fires only on publication: a draft is
-- allowed to be incomplete, which is what a draft is for.

CREATE OR REPLACE FUNCTION trg_offensive_missions_are_authorised()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.status = 'draft' OR NEW.rules_of_engagement_url IS NOT NULL THEN
        RETURN NEW;
    END IF;

    IF EXISTS (
        SELECT 1 FROM mission_types mt
         WHERE mt.id = NEW.mission_type_id
           AND mt.slug IN ('sec_pentest_web', 'sec_pentest_mobile',
                           'sec_pentest_infra', 'sec_purple_exercise')
    ) THEN
        RAISE EXCEPTION
            'an offensive security mission cannot leave draft without '
            'rules_of_engagement_url';
    END IF;

    RETURN NEW;
END $$;

COMMENT ON FUNCTION trg_offensive_missions_are_authorised() IS
    'Refuses to publish an offensive security engagement with no written '
    'authorisation. A trigger rather than a CHECK because the mission type is '
    'a row in another table.';

CREATE TRIGGER trg_missions_offensive_authorisation
    BEFORE INSERT OR UPDATE OF status, mission_type_id, rules_of_engagement_url
    ON missions
    FOR EACH ROW EXECUTE FUNCTION trg_offensive_missions_are_authorised();

COMMENT ON COLUMN missions.rules_of_engagement_url IS
    'The written authorisation for an engagement that touches a live system. '
    'Required for the offensive mission types by a CHECK rather than by a '
    'policy document, for the reason 0550 gives about slices.';

COMMENT ON COLUMN missions.allows_public_disclosure IS
    'Whether the person who did the work may describe it afterwards. '
    'Deliberately not folded into ip_terms: a client can own the report and '
    'still allow a redacted write-up, and that pair is common.';

COMMENT ON COLUMN missions.required_credentials IS
    'Credential names an applicant must have declared. Declared, not verified '
    '— the application response says which, because a client asking for an '
    'OSCP is entitled to know nobody has checked it.';

-- ═══════════════════════════════════════════════════════════════════
-- The deal shape that was missing
-- ═══════════════════════════════════════════════════════════════════
--
-- Restated in full rather than added to, because a CHECK cannot be extended —
-- the failure 0305 documented. The four existing values are unchanged and the
-- fifth is new.

ALTER TABLE missions
    DROP CONSTRAINT missions_ip_terms_check,
    ADD CONSTRAINT missions_ip_terms_check CHECK (ip_terms IN (
        'full_ownership_client',
        'open_source_output',
        'retain_reusable_components',
        'dual_license',
        -- The researcher keeps the report and the client gets a right to use
        -- it. Rare, real, and usually priced lower.
        'licence_to_client'
    ));
