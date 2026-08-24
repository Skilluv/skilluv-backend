-- What a reviewer reads for, per family of communication trade.
--
-- Migration 0180 built the table; 0211 gave `ai` its grids and stated the
-- rule: a domain with no default sends work to the verifier with the
-- instructions alone — the model is asked whether the work is good with no
-- statement of what good means, and answers anyway.
--
-- ## The common criteria are the ones the whole domain shares
--
-- Communication is judged first on whether it *worked on somebody*. A page,
-- a talk or a video is not good in itself; it is good when a reader who
-- arrived with a question left with an answer. That is why service to the
-- reader sits at the top, before craft.
--
-- Second on accuracy. This domain is the one where a confident error travels
-- furthest: a wrong tutorial is copied into a thousand projects, and the
-- author never hears about it.
--
-- Third on attribution. An unattributed paragraph, an uncredited screenshot,
-- an undisclosed sponsorship — each is a different failure and all three make
-- the piece unusable rather than merely weaker, which is the same structure
-- as the audio provenance criterion of 0405.
--
-- ## The one about AI says something the other domains' version does not
--
-- Every domain's grid says a generative tool is accepted and hiding it is
-- not. Here the tool produces the artefact itself rather than helping to
-- build it, so the criterion has to be sharper: what matters is not whether a
-- model wrote a draft, it is whether a person checked every claim in it. A
-- disclosed piece full of unverified assertions fails this grid; a disclosed
-- piece whose every claim was tested passes it.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('communication', NULL, 'Communication — common criteria', '[
  {"criterion": "Service to the reader", "looks_like": "Somebody arrived with a question and left with an answer. A brilliant text that leaves the reader where it found them has missed."},
  {"criterion": "Technical accuracy", "looks_like": "Every claim is true and checkable. Every example was executed. This is the domain where a confident error travels furthest: it gets copied, and the author never finds out."},
  {"criterion": "Structure", "looks_like": "At every point the reader knows where they are and what comes next. Headings say what is under them; the table of contents alone reveals the plan."},
  {"criterion": "Level announced and held", "looks_like": "Prerequisites are stated at the top, and nothing beyond them is assumed afterwards. The unannounced leap is the first cause of abandonment."},
  {"criterion": "Attribution and sources", "looks_like": "What comes from elsewhere is cited with a reachable link; screenshots, excerpts and data have an origin. A paid partnership is declared at the top, not in a footer."},
  {"criterion": "Accessibility", "looks_like": "Alt text, contrast, captions on video, a transcript on audio. A resource part of the audience cannot consume was not published for that audience."},
  {"criterion": "Transparency about AI", "looks_like": "Use of a generative tool is declared. It is accepted; what is not is a claim no human verified. The line is there, not on the tool."}
]'),

('communication', 'documentation', 'Documentation — review grid', '[
  {"criterion": "Page type respected", "looks_like": "Tutorial, how-to, reference or explanation: the page is one of them and stays one. A tutorial that detours into architecture loses both readers."},
  {"criterion": "Complete path", "looks_like": "From the first prerequisite to an observable result, with no gap. Verified on a clean machine, not on the author''s."},
  {"criterion": "Runnable examples", "looks_like": "Copy, paste, it works. Dependency versions stated. An example that does not compile costs more than no example."},
  {"criterion": "Reference completeness", "looks_like": "Every parameter, every return, every error. Edge cases and defaults are stated."},
  {"criterion": "Findability", "looks_like": "Headings, anchors, index, cross-links. A reader landing mid-page from a search engine can tell where they are."},
  {"criterion": "Maintenance", "looks_like": "The page lives with the code: versioned with it, reviewed in review, and whatever goes stale is dated or deleted."}
]'),

('communication', 'advocacy', 'Speaking and content — review grid', '[
  {"criterion": "Promise kept", "looks_like": "The title says what is inside, and it is inside. A title that promises more than the content costs you the next audience."},
  {"criterion": "Progression", "looks_like": "It moves: an idea, its demonstration, its consequence. No ten-minute plateau where nothing new is said."},
  {"criterion": "Demonstration", "looks_like": "Something runs in front of the audience, and the fallback path exists. A demo that breaks with no plan B is a demo that was not prepared."},
  {"criterion": "Production quality", "looks_like": "Intelligible voice, code legible from the back of the room or on a phone, editing that does not stretch a ten-second operation."},
  {"criterion": "Technical honesty", "looks_like": "Teaching shortcuts are owned and flagged. Simplifying is not distorting."},
  {"criterion": "What you take away", "looks_like": "One precise thing to do, read or try on the way out. Resources are linked, not cited from memory."},
  {"criterion": "Relationship to the audience", "looks_like": "Questions were answered, including the ones that disagreed. An abandoned comment section is part of the delivery."}
]'),

('communication', 'translation', 'Translation — review grid', '[
  {"criterion": "Accuracy", "looks_like": "The technical meaning survives. No false friend, no approximation on a term that decides a behaviour."},
  {"criterion": "Naturalness", "looks_like": "It reads as written in the target language, not as translated. Calqued constructions show up in the first sentence."},
  {"criterion": "Terminology", "looks_like": "One term, one translation, everywhere. The glossary is supplied or followed; two words for one concept make the translation harder than the original."},
  {"criterion": "What is not translated", "looks_like": "API names, keywords, the program''s own error messages: left as they are, and that choice is consistent throughout."},
  {"criterion": "Adaptation", "looks_like": "Date and number formats, examples, screenshots, reading direction. What has to change changes, and nothing else does."},
  {"criterion": "Completeness", "looks_like": "Nothing is skipped. An untranslated section is marked as such rather than left in the source language without a word."},
  {"criterion": "Source tracking", "looks_like": "The version of the original that was translated is recorded, so a maintainer knows what is left to redo when it moves."}
]'),

('communication', 'research-writing', 'Research writing — review grid', '[
  {"criterion": "Question asked", "looks_like": "You know what is being asked and why it arises. A whitepaper with no question is a brochure."},
  {"criterion": "Prior art", "looks_like": "What already exists is read, cited and situated. Announcing something new without looking at the old is the most frequent fault."},
  {"criterion": "Reproducible method", "looks_like": "A stranger can repeat the measurement and get the same result: protocol, data, versions, hardware."},
  {"criterion": "Honesty about figures", "looks_like": "Uncertainties given, axes not misleading, unfavourable cases shown as fully as favourable ones."},
  {"criterion": "Limits stated", "looks_like": "What the work does not prove is written by the author, not left for the reader to discover."},
  {"criterion": "Citations", "looks_like": "Every borrowed claim carries a reachable reference. A dead link is a missing citation."},
  {"criterion": "Conflicts of interest", "looks_like": "Funding, employer, product evaluated: stated at the top. An industry report paid for by an actor in that industry reads differently, and the reader is entitled to that context."}
]');
