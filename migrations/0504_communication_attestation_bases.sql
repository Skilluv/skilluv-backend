-- What a communication attestation can rest on.
--
-- Six rows in the table 0406 created, and one renaming of what the backlog
-- asked for.
--
-- ## `content_published_viral` is not a basis
--
-- Ticket F-08 asked for `content_published_viral`, issued when a video or a
-- post crosses ten thousand views. Two things are wrong with that, and both
-- are the kind that only show up later.
--
-- A basis whose name encodes a threshold becomes false when the threshold
-- moves. Ten thousand views on a niche Rust tutorial in Wolof and ten
-- thousand on a JavaScript video are not the same achievement, and the day
-- somebody argues for lowering the bar the platform has to choose between an
-- inaccurate name and reissuing everybody's attestations.
--
-- Worse, the number itself is usually the person's own word. Migration 0415
-- established the rule for this domain's cousins: figures from platforms with
-- no usable API are accepted, marked as declared, and discounted. An
-- attestation is the one artefact that must not carry a declared number as if
-- it were checked.
--
-- So the basis is `communication_content_published`, which says what a
-- stranger can verify by following the link: this person published this. The
-- audience is a portfolio figure, it is marked as declared or fetched, and
-- the badge is where a threshold belongs — a badge can be rewritten without
-- touching what anybody was already told.
--
-- ## Why the docs basis says "upstream"
--
-- `communication_docs_contribution` is issued on a documentation change
-- accepted by a project the author does not control. Documentation written
-- for one's own repository is real work and is not this: the claim being made
-- is that somebody else's maintainer read it and took it.
--
-- ## The seventh is editorial
--
-- `featured_communicator`, like `featured_coder` and `featured_audio_creator`
-- before it. It names a person rather than an artefact, so it carries no
-- deliverable.

INSERT INTO attestation_bases
    (basis, skill_domain, title, description, requires_deliverable, sort_order) VALUES

('communication_docs_contribution', 'communication',
 'Documentation contribution accepted',
 'A documentation change accepted by a project the person does not control.',
 TRUE, 410),

('communication_talk_delivered', 'communication',
 'Talk delivered',
 'A talk given to an audience, with its recording or its slides published.',
 TRUE, 420),

('communication_content_published', 'communication',
 'Technical content published',
 'A video, article, episode or stream published at a public address.',
 TRUE, 430),

('communication_translation_validated', 'communication',
 'Translation validated',
 'A technical translation reviewed by somebody who reads both languages, and accepted upstream.',
 TRUE, 440),

('communication_research_published', 'communication',
 'Research writing published',
 'A whitepaper, paper or external specification published, with its method and its sources.',
 TRUE, 450),

('featured_communicator', 'communication',
 'Featured',
 'Communication work picked out by the editors as exemplary.',
 FALSE, 460);
