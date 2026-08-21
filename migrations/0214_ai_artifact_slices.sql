-- Slices that produce a model, a dataset or a paper.
--
-- ## The same shape as 0181, for the same reason
--
-- `slice_type` says which surface the work lives on, and the ingestion, the
-- validation queues and the UI all switch on it. `ai_subtype` says what the
-- finished artefact is, and is NULL unless the slice is an `ai_artifact`.
--
-- ## Where the artefact lives
--
-- Not here. The backlog planned a MinIO bucket with fifty-gigabyte multipart
-- uploads for model weights, and that is the wrong place to spend: weights
-- already have homes — HuggingFace, Kaggle, GitHub Releases — that are free,
-- faster, and where the people who would use the model already look. Hosting
-- a copy would cost real money to make an artefact *less* discoverable.
--
-- So `ai_external_hosting_url` is the artefact, and it is required for the
-- subtypes where a claim is worthless without it. A model nobody can download
-- is a sentence; the URL is what makes it opposable, which is the whole
-- premise of the platform.

ALTER TABLE project_slices
    DROP CONSTRAINT IF EXISTS project_slices_slice_type_check;

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_slice_type_check
    CHECK (slice_type IN (
        'github_issue', 'figma_frame', 'game_level', 'game_asset',
        'sec_target', 'cli_task', 'design_token', 'documentation',
        'code_artifact', 'ai_artifact',
        'other'
    ));

ALTER TABLE project_slices
    ADD COLUMN ai_subtype VARCHAR(30),
    -- Plural for the same reason `code_languages` is: a training pipeline in
    -- PyTorch exported to ONNX and served with vLLM is one slice, and forcing
    -- a single name would either lose two thirds of it or invent artefacts
    -- that were never separate.
    ADD COLUMN ai_frameworks TEXT[] NOT NULL DEFAULT '{}',
    -- Where the artefact actually lives: a HuggingFace repository, a Kaggle
    -- dataset, an arXiv entry, a deployed endpoint.
    ADD COLUMN ai_external_hosting_url TEXT,
    -- Parameter count, when the artefact is a model. Says more about what it
    -- takes to run than any adjective would.
    ADD COLUMN ai_model_size_params BIGINT
        CHECK (ai_model_size_params IS NULL OR ai_model_size_params > 0);

ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_ai_subtype_values
    CHECK (ai_subtype IS NULL OR ai_subtype IN (
        'data_pipeline',      -- ETL code and the dataflow it documents
        'ml_model',           -- weights, training code, evaluation report
        'llm_agent',          -- prompts, tools, evals, guardrails
        'dataset',            -- a published dataset and its card
        'ai_service_api',     -- a deployed service with its API documentation
        'ai_research_paper'   -- a paper and the code that supports it
    ));

-- A subtype only means something on an AI artefact, and an AI artefact
-- without one is a slice nobody can attest against.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_ai_subtype_belongs_to_ai_artifact
    CHECK (
        (slice_type = 'ai_artifact' AND ai_subtype IS NOT NULL)
        OR (slice_type <> 'ai_artifact' AND ai_subtype IS NULL)
    );

-- A model, a dataset, a paper or a running service has to say where it is.
-- Without the address the claim cannot be opened by a stranger, which is the
-- only thing that separates it from a sentence. A data pipeline and an agent
-- system are exempt: both are normally a repository, and `fork_repo_url`
-- already carries that.
ALTER TABLE project_slices
    ADD CONSTRAINT project_slices_hosted_ai_artifact_says_where
    CHECK (
        ai_subtype IS NULL
        OR ai_subtype NOT IN ('ml_model', 'dataset', 'ai_research_paper',
                              'ai_service_api')
        OR ai_external_hosting_url IS NOT NULL
    );

COMMENT ON COLUMN project_slices.ai_subtype IS
    'What the finished artefact is, for ai_artifact slices. slice_type says '
    'which surface the work lives on; this says what comes out of it.';

COMMENT ON COLUMN project_slices.ai_external_hosting_url IS
    'Where the artefact lives — HuggingFace, Kaggle, arXiv, a deployed '
    'endpoint. Skilluv does not host weights: they already have free homes '
    'where the people who would use them look, and a copy here would cost '
    'money to make the work less findable.';

COMMENT ON COLUMN project_slices.ai_frameworks IS
    'Every framework the slice touches. Plural because a slice that spans two '
    'is one slice, and splitting it would invent an artefact.';

CREATE INDEX idx_project_slices_ai_subtype
    ON project_slices (ai_subtype)
    WHERE ai_subtype IS NOT NULL;

CREATE INDEX idx_project_slices_ai_frameworks
    ON project_slices USING gin (ai_frameworks);
