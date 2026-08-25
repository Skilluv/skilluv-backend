-- What a reviewer looks for, per family of education trade.
--
-- Migration 0180 built the table; 0211 gave `ai` its grids and stated the
-- rule: a domain with no default sends work to the verifier with the
-- instructions alone — the model is asked whether the work is good with no
-- statement of what good means, and answers anyway.
--
-- ## The common criteria, and the one this domain is unusual for
--
-- Education is judged first on whether anybody learned anything, which sounds
-- obvious and is the criterion most often replaced by a proxy. A cohort where
-- everybody enjoyed themselves and nobody can do the thing has failed. So
-- "measured outcome" sits at the top, and satisfaction is named separately
-- and lower — it is a real signal about whether people will come back, and it
-- is not evidence of learning.
--
-- The unusual one is the last: learner data. This is the only domain on the
-- platform whose artefacts routinely contain facts about identifiable third
-- parties who are not members here, are sometimes minors, and never asked to
-- be evidence in somebody's portfolio. A cohort report naming students, a
-- testimonial screenshot with a face, an assessment spreadsheet: each is a
-- delivery that cannot be accepted as submitted. That makes it a review
-- criterion rather than a policy note, in the same way provenance is one for
-- audio (0405): it is not that the work is weaker, it is that it cannot be
-- published at all.

INSERT INTO review_grids (domain, reviewer_group, display_name, criteria) VALUES

('education', NULL, 'Education — common criteria', '[
  {"criterion": "Somebody learned something", "looks_like": "There is evidence of a change in what learners can do: a before and after, a completed project, a measured assessment. A session everybody enjoyed and nobody can act on has failed at the only thing it was for."},
  {"criterion": "Objectives stated and met", "looks_like": "What the learner would be able to do was written before it started, in observable terms, and the delivery went there. \"Understands recursion\" is not an objective; \"writes a recursive traversal and predicts its depth\" is."},
  {"criterion": "The level was right", "looks_like": "Prerequisites announced and honoured, no unannounced jump, and nothing spent on what the room already knew. The silent jump is the single most common failure here."},
  {"criterion": "Learners did something", "looks_like": "Time spent practising rather than watching. A three-hour workshop with twenty minutes of hands-on is a talk with exercises attached."},
  {"criterion": "Materials somebody else can use", "looks_like": "Slides, exercises, solutions and the environment setup, in a state where another trainer could run it. Work that only its author can deliver is not an artefact."},
  {"criterion": "Satisfaction, read as what it is", "looks_like": "Reported and taken seriously as a signal about whether people return — and never presented as evidence that anybody learned."},
  {"criterion": "Learner data handled", "looks_like": "Names, faces, marks and messages are anonymised or carry explicit consent, and nothing identifiable about a minor appears at all. A delivery that exposes a learner is refused whatever else it does."},
  {"criterion": "Transparency about AI", "looks_like": "Use of a generative tool in preparing materials is declared. It is accepted; what is not is an exercise nobody ran or a solution nobody checked."}
]'),

('education', 'teaching', 'Teaching — review grid', '[
  {"criterion": "The room stayed with it", "looks_like": "Attention held, questions came, and the people who went quiet were noticed. Evidence rather than impression: participation, checkpoints, who finished."},
  {"criterion": "Explanation", "looks_like": "One unfamiliar thing at a time, an example before the abstraction, and the vocabulary controlled. Jargon introduced is jargon defined."},
  {"criterion": "Practice", "looks_like": "Exercises that fail instructively, are finishable in the time given, and cannot be completed by copying the previous one."},
  {"criterion": "Handling what was not prepared", "looks_like": "A question outside the plan is answered, deferred honestly, or admitted. A demonstration that breaks is debugged out loud rather than skipped."},
  {"criterion": "Diagnosis", "looks_like": "A stuck learner is read correctly: missing prerequisite, misread instruction, broken environment, or afraid to ask. Four problems that look identical from the front of the room."},
  {"criterion": "Support that is withdrawn", "looks_like": "Help given so it can stop. A learner who can only work with the teacher present has been carried, not taught."},
  {"criterion": "Outcomes measured", "looks_like": "Completion, assessment results, or something the learner built. Numbers with a method behind them, not a feeling about how it went."}
]'),

('education', 'curriculum', 'Curriculum — review grid', '[
  {"criterion": "Objectives", "looks_like": "Observable, testable, and written for the learner rather than about the material. Every module has them and they add up to what the programme claims."},
  {"criterion": "Progression", "looks_like": "Each step is reachable from the last. Prerequisites are explicit, including the ones an expert has forgotten they know."},
  {"criterion": "Load", "looks_like": "One new thing at a time. A module that introduces a language, a framework and a toolchain at once teaches none of the three."},
  {"criterion": "Variety of activity", "looks_like": "Reading, doing, explaining, breaking. A programme that is one mode from end to end loses most of a room."},
  {"criterion": "Assessment alignment", "looks_like": "What is assessed is what the objectives claimed. A rubric that rewards something else is a curriculum that teaches something else."},
  {"criterion": "Deliverable by somebody else", "looks_like": "A second trainer could run it: facilitator notes, timings, solutions, environment, and what to do when a session runs long."},
  {"criterion": "Maintenance", "looks_like": "Versioned, dated, and honest about what goes stale. A curriculum with no expiry on its tool versions is one that breaks silently."},
  {"criterion": "Reach", "looks_like": "Works on a bad connection, with a screen reader, and for somebody who could not attend live. Designed in rather than added after."}
]');
