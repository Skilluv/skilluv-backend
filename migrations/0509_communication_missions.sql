-- Paid communication work, on the mission table that already exists.
--
-- ## Why there is no `communication_missions`
--
-- Ticket M-01 asked for one, with its own application flow and its own
-- payment path. Migration 0301 refused that for AI and 0413 refused it again
-- for audio; the reason has not changed. `missions` is keyed by
-- `skill_domain` and already carries the applications, the payment models,
-- the IP terms, the commission and the state machine. A second table means a
-- second answer to "how many missions has this person finished", and both get
-- quoted.
--
-- Every field the ticket listed exists:
--
--   * `target_language` — `missions.target_languages`, an array since 0192,
--     which is the right shape: a translation commission routinely names
--     three.
--   * `deliverable_format` — `mission_deliverable_formats`, a table since
--     0413. Six rows are added below.
--   * `nda_required`, `ip_terms`, `budget_eur`, `payment_model`,
--     `apply_deadline` — all present, the last as
--     `applications_close_at`.
--   * `per_deliverable` payment, asked for by M-03 — a value of
--     `missions.payment_model` since 0192.
--   * fifteen per cent commission — the column default.
--
-- `mission_applications` already holds `portfolio_urls`, `expertise` and
-- `past_similar_missions`, which is the whole of what M-02 asked for.
--
-- ## Licensing scope becomes compulsory here too
--
-- 0413 added `licensing_scope` for audio and made it mandatory there, with
-- the reasoning that ownership and licence are different questions and the
-- licence is where the disputes are. A commissioned article is the same
-- situation: the writer who keeps the copyright still has to say whether the
-- client may syndicate it, translate it, or run it under somebody else's
-- byline. A commission with no stated scope is the most common way this goes
-- wrong, in exactly the way 0413 described for music.
--
-- ## `podcast_episode_guest` is a mission and not a favour
--
-- It is the one on the ticket's list that looks like it should not be paid,
-- and it is kept for that reason. Appearing on a company's podcast is
-- promotional work for the company, it takes preparation, and treating it as
-- an honour rather than a commission is how this domain gets asked to work
-- for exposure.

INSERT INTO mission_deliverable_formats (slug, skill_domain, name, description, sort_order) VALUES
    ('docs_set', 'communication', 'Page set',
     'Pages delivered into the client''s repository, in their format, reviewed and wired into their navigation.', 310),
    ('published_article', 'communication', 'Published article',
     'An article published at an agreed address, with its byline and its sources.', 320),
    ('video_package', 'communication', 'Video and sources',
     'The edited video, plus the script, the captions and the source files.', 330),
    ('translated_bundle', 'communication', 'Translated bundle',
     'The translated files in their original format, the glossary used, and the source version that was translated.', 340),
    ('whitepaper_document', 'communication', 'Research document',
     'The document, its data and its method, in a form a reader can replay.', 350),
    ('talk_package', 'communication', 'Talk and materials',
     'The talk as given, its recording, its slides and the resources cited.', 360);

INSERT INTO mission_types (slug, skill_domain, name, description, sort_order) VALUES
    ('comm_docs_authoring', 'communication', 'Documentation authoring',
     'Writing or rebuilding a documentation set: getting started, guides, reference.', 310),
    ('comm_devrel_content_pack', 'communication', 'DevRel content pack',
     'A coherent series — articles, demonstrations, workshops — around one technology.', 320),
    ('comm_conference_talk', 'communication', 'Commissioned talk',
     'A talk or workshop given on behalf of an organisation, preparation included.', 330),
    ('comm_video_tutorial_series', 'communication', 'Video tutorial series',
     'Several videos that follow one another, from script to edit.', 340),
    ('comm_translation_project', 'communication', 'Translation project',
     'Carrying a documentation set into one or more languages, glossary included.', 350),
    ('comm_whitepaper_authoring', 'communication', 'Whitepaper authoring',
     'A piece whose value rests on its method: question, protocol, results, limits.', 360),
    ('comm_podcast_episode_guest', 'communication', 'Guest appearance',
     'Appearing on a podcast or a stream on behalf of an organisation. Paid because it is prepared promotional work, not an honour.', 370);

ALTER TABLE missions
    ADD CONSTRAINT missions_communication_states_its_licensing_scope CHECK (
        skill_domain <> 'communication' OR licensing_scope IS NOT NULL
    );

COMMENT ON CONSTRAINT missions_communication_states_its_licensing_scope ON missions IS
    'A commissioned article, video or translation has to say what the client '
    'may do with it. Same reasoning as the audio constraint of 0413: '
    'ownership and licence are different questions, and the licence is where '
    'the disputes are.';
