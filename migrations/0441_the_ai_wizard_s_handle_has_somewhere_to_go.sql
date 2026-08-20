-- The AI wizard asks for a HuggingFace username. Give it a platform to be.
--
-- `huggingface_username` has been a question since the AI wizard shipped, and
-- the answer went into the profile blob and was read by nothing. The audio
-- wizard's two handles do better: they become rows in `user_external_portfolios`
-- so a reader gets a link they can follow. Wiring the AI handle the same way
-- needs the platform to exist, because 0415 replaced the platform CHECK with a
-- foreign key onto this table -- and the insert that misses is logged and
-- dropped, so without this row the handle would go on quietly vanishing.
--
-- `has_public_api` is TRUE: the hub serves model and dataset counts without a
-- key. The counts are not read yet; the column says what is possible, not what
-- is done, and saying FALSE here would close the question.

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, items_label, reach_label, has_public_api, sort_order) VALUES
    ('huggingface', 'ai', 'HuggingFace', 'modèles', 'téléchargements', TRUE, 310)
ON CONFLICT (slug) DO NOTHING;
