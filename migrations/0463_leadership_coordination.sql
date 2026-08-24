-- What a leadership document coordinates.
--
-- ## The thing this makes possible
--
-- A quarterly roadmap that touches five projects is one artefact and five
-- commitments. Without a link table it is a document with five project names
-- written inside it, and nothing on those projects knows it exists — so the
-- steward of the fourth one finds out when somebody asks why their quarter
-- was planned for them.
--
-- ## Why the link carries a kind
--
-- "This roadmap mentions that project" and "this roadmap commits that project
-- to a date" are different statements, and only the second is something the
-- project's steward should be notified about. A link with no kind would have
-- to be treated as the stronger one, which turns every reference into an
-- obligation and teaches people to stop linking.

CREATE TABLE leadership_artifact_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The leadership artefact doing the coordinating. Its slice, because the
    -- slice is what carries the trade, the review and the attestation.
    leadership_slice_id UUID NOT NULL
        REFERENCES project_slices(id) ON DELETE CASCADE,
    linked_project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,

    link_kind VARCHAR(20) NOT NULL CHECK (link_kind IN (
        -- The document plans work this project will do. The strong one: the
        -- project's steward is told.
        'commits',
        -- The document's own plan depends on this project delivering
        -- something. Also strong, in the other direction.
        'depends_on',
        -- The document coordinates this project with others without
        -- committing it to anything new.
        'coordinates',
        -- The document refers to this project as context. The weak one, and
        -- the default when somebody is unsure.
        'references'
    )),

    -- What is being committed, depended on or coordinated, in one line.
    -- Required on the two strong kinds — see the constraint — because a
    -- commitment nobody wrote down is a commitment nobody can dispute.
    note TEXT,

    -- Whether the project's steward has seen it. NULL on the weak kinds,
    -- which nobody is asked to acknowledge.
    acknowledged_by UUID REFERENCES users(id) ON DELETE SET NULL,
    acknowledged_at TIMESTAMPTZ,

    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- One link per pair per kind. The same roadmap can both commit a project
    -- and depend on it, which is a real situation and two rows.
    UNIQUE (leadership_slice_id, linked_project_id, link_kind),

    CONSTRAINT a_commitment_says_what CHECK (
        link_kind NOT IN ('commits', 'depends_on')
        OR (note IS NOT NULL AND btrim(note) <> '')
    ),
    CONSTRAINT acknowledgement_is_complete CHECK (
        (acknowledged_at IS NULL) = (acknowledged_by IS NULL)
    )
);

COMMENT ON TABLE leadership_artifact_links IS
    'What a leadership document coordinates. The kind matters: "mentions" and '
    '"commits to a date" are different statements, and treating every '
    'reference as an obligation teaches people to stop linking.';

COMMENT ON COLUMN leadership_artifact_links.acknowledged_at IS
    'Set by the linked project''s steward. What turns a plan written about '
    'somebody into a plan agreed with them.';

CREATE INDEX idx_leadership_links_by_slice
    ON leadership_artifact_links (leadership_slice_id);

-- What a project's steward reads: everything planning their work that they
-- have not yet seen.
CREATE INDEX idx_leadership_links_unacknowledged
    ON leadership_artifact_links (linked_project_id, created_at)
    WHERE acknowledged_at IS NULL AND link_kind IN ('commits', 'depends_on');

-- A link hangs off a leadership artefact and off nothing else. Enforced here
-- rather than in the service, because the notification the strong kinds send
-- is addressed by reading this row: a link on a code slice would notify a
-- steward about a document that is not a plan.
CREATE FUNCTION trg_leadership_link_source_is_an_artifact() RETURNS TRIGGER AS $$
DECLARE
    kind VARCHAR;
BEGIN
    SELECT slice_type INTO kind FROM project_slices WHERE id = NEW.leadership_slice_id;

    IF kind <> 'leadership_artifact' THEN
        RAISE EXCEPTION
            'slice % is a %, and only a leadership_artifact coordinates other projects',
            NEW.leadership_slice_id, kind
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_leadership_link_source_is_an_artifact
    BEFORE INSERT OR UPDATE OF leadership_slice_id ON leadership_artifact_links
    FOR EACH ROW EXECUTE FUNCTION trg_leadership_link_source_is_an_artifact();

-- ═══════════════════════════════════════════════════════════════════
-- What a leadership artefact reaches, from the project's side
-- ═══════════════════════════════════════════════════════════════════
--
-- The dashboard the backlog asks for (leadership/W-03), as a view rather than
-- a service query, so the same shape is available to the API, to the admin
-- screens and to anybody reading the database directly.

CREATE VIEW leadership_coordination_reach AS
SELECT ps.id AS leadership_slice_id,
       ps.title,
       ps.leadership_subtype,
       ps.redaction_state,
       count(l.id) AS projects_linked,
       count(l.id) FILTER (WHERE l.link_kind = 'commits') AS projects_committed,
       count(l.id) FILTER (
           WHERE l.link_kind IN ('commits', 'depends_on')
             AND l.acknowledged_at IS NOT NULL
       ) AS commitments_acknowledged,
       count(l.id) FILTER (
           WHERE l.link_kind IN ('commits', 'depends_on')
             AND l.acknowledged_at IS NULL
       ) AS commitments_outstanding
  FROM project_slices ps
  LEFT JOIN leadership_artifact_links l ON l.leadership_slice_id = ps.id
 WHERE ps.slice_type = 'leadership_artifact'
 GROUP BY ps.id;

COMMENT ON VIEW leadership_coordination_reach IS
    'How far a leadership document reaches, and how much of that reach has '
    'been agreed rather than announced. `commitments_outstanding` is the '
    'number a reviewer reads first.';
