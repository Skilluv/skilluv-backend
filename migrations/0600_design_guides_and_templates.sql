-- The design domain's guides and templates.
--
-- ## The gap this closes
--
-- Design was the only opened domain with **zero rows** in `content_guides`.
-- Every other one carries its onboarding, its toolkit and its templates:
-- audio, communication, education, game, leadership and quality are complete,
-- code and ai and ops and security carry most of it. Design carried nothing.
--
-- Meanwhile `docs/design/ONBOARDING.md`, `TOOLKIT.md`, `BRIEF-TEMPLATES.md`
-- and `WRITEUP-TEMPLATES.md` all exist and are written. The content was never
-- the problem — nobody had turned it into rows, so `GET /api/guides?
-- skill_domain=design` answered an empty list and the front's `/design/toolkit`
-- and `/design/onboarding` pages read it (SKI-186).
--
-- This is that port. The words come from those four files rather than from a
-- fresh invention: a guide that disagrees with the document it was written
-- from is worse than no guide, because two sources both claim to be current.
--
-- ## Thirteen families, not twenty-six trades
--
-- One onboarding guide and one brief template per **reviewer group**, which is
-- how the trades are already grouped everywhere else — by who is competent to
-- review them. Twenty-six of each would be twenty-six documents saying almost
-- the same thing, and the difference between `design-motion-2d` and
-- `design-motion-3d` lives in the brief, not in the guidance.
--
-- ## English
--
-- The repository's content language, and the fallback chain in
-- `routes::guides` is "asked for, then English, then French" — so a French
-- reader gets these pages rather than nothing. The French source documents
-- stay where they are; a `fr` row per slug can be added later without
-- touching this.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES

-- ═══════════════════════════════════════════════════════════════════
-- Arriving
-- ═══════════════════════════════════════════════════════════════════

('design-welcome', 'onboarding', 'design', NULL, 'en',
 'Welcome, designer', 'Where to start, whichever of the twenty-six trades is yours.',
 'Skilluv Design is compagnonnage for design work: real briefs, critiqued by somebody competent in your trade, proven by attestations you can show.

Declare up to three trades — the cap is deliberate, because somebody who declares twelve has said nothing and the filtering stops filtering. Declaring is not proving. What proves is a validated deliverable in that trade.

**Twenty-six trades exist; public communication starts with eight.** Product, design system, editorial web, mobile, iconography, UX writing, dataviz and design ops are the ones that already have reviewers and real projects. The other eighteen are open in the database: a sound designer arriving today can declare their trade and hand work in, and may wait for a reviewer. That is written down rather than hidden, because an empty ecosystem promise is worse than a domain announced as young.

**What surprises people who come from elsewhere:** taking three rounds is worth more here than getting it right first time. The craft score counts work carried to validation after three rounds or more, separately. Coming back after being told the direction was wrong is the hardest skill in the trade.', 10),

-- ═══════════════════════════════════════════════════════════════════
-- One onboarding guide per review family
-- ═══════════════════════════════════════════════════════════════════

('design-onboarding-product', 'onboarding', 'design', 'product', 'en',
 'Onboarding: product, systems, conversational',
 'For product, design system and AI-conversational designers.',
 'A screen on its own is not a deliverable. What is reviewed is a journey from its entry point to its end, and the states nobody enjoys drawing: empty, loading, error, permission refused. A mock-up that shows only the nominal case cannot be built from.

For a design system, two questions decide the review: which products will use it, and who maintains it. A component drawn without saying how it behaves when its text doubles in length is a component that breaks on the first translation.

Start small. A first challenge exists to teach you the critique loop, not to impress anybody.', 20),

('design-onboarding-web', 'onboarding', 'design', 'web', 'en',
 'Onboarding: web and editorial',
 'For web designers and editorial web designers.',
 'The reading hierarchy is the work. What must be seen first, and what the page does at the real volume of content — including the worst case, a ninety-character heading, an article three times longer than the sample.

Composing on invented text that is shorter than the real thing is the pitfall of this family, and it is the one that costs a round. Ask for the real content, or the worst case of it.

Weight is a design constraint here, not an engineering afterthought: a page budget, and what the layout does with images off.', 30),

('design-onboarding-mobile', 'onboarding', 'design', 'mobile', 'en',
 'Onboarding: mobile',
 'For iOS, Android and cross-platform designers.',
 'Three things are checked before anything aesthetic: the thumb zone and one-handed use, behaviour offline and on a slow connection, and the real screen sizes — including the small ones.

Offline is not negotiable for a West African audience. A flow that assumes a live connection is a flow that fails where most of this community is.

The pitfall of this family is mocking up on a 6.7-inch screen only. Platform conventions are either respected or knowingly departed from, and the brief should say which.', 40),

('design-onboarding-motion', 'onboarding', 'design', 'motion', 'en',
 'Onboarding: motion and video',
 'For motion UI, 2D, 3D and video designers.',
 'Deliver the render **and** the project. A render alone cannot be iterated on, and the second round is where this trade is actually judged.

Say what triggers the animation, for interface motion. Say whether there is sound or whether it is muted by default. And handle reduced motion: `prefers-reduced-motion` is on somebody''s machine for a medical reason, and ignoring it is the pitfall of this family.

Weight: a 4K uncompressed render exceeds any reasonable limit. Deliver 1080p H.264 and keep the project beside it.', 50),

('design-onboarding-brand', 'onboarding', 'design', 'brand', 'en',
 'Onboarding: brand, typography, verbal',
 'For brand identity, type and naming designers.',
 'A mark is judged at its worst case of reproduction, not at its best: one colour, small, screen-printed, embroidered. Presenting an identity only large on a white background is the pitfall of this family and it hides everything that matters.

Name the real supports where the brand will appear, from the most constrained to the freest. Say what already exists and what is being kept.

**The licence trap is the most expensive in the domain.** A desktop-licensed font is not deliverable to a client: the licence is yours, not theirs, and they receive the invoice. Use libre faces — Google Fonts, Fontshare, Velvetyne — or have the client buy the licence in their own name before delivery.', 60),

('design-onboarding-illustration', 'onboarding', 'design', 'illustration', 'en',
 'Onboarding: illustration, icons, characters',
 'For illustrators, icon designers and character designers.',
 'Set coherence is what is reviewed: how many pieces, on what grid, at what stroke weight. A single beautiful drawing is not the deliverable when the brief asks for a family.

State the render sizes, and the smallest one. Drawing an icon that is magnificent at 128px and a smudge at 16px is the pitfall of this family, and the smallest size is where it is caught.

Name your files the way somebody else will need to find them.', 70),

('design-onboarding-dataviz', 'onboarding', 'design', 'dataviz', 'en',
 'Onboarding: data visualization',
 'For dataviz designers.',
 'Ask for the real data, or a representative set — **with its outliers**. A visualization calibrated on clean invented numbers falls apart on the first real load, and that is the pitfall of this family.

Say which question the visualization answers. A chart that answers no stated question is decoration.

Readability without colour is a structural constraint here, not an accessibility add-on: colour is one encoding among several, and it is the one some readers do not receive.', 80),

('design-onboarding-ux-writing', 'onboarding', 'design', 'ux-writing', 'en',
 'Onboarding: UX writing',
 'For interface writers and content designers.',
 'The error cases are the work. Writing the happy labels and leaving the errors in technical English is the pitfall of this family, and errors are where a person is most likely to be reading carefully and least likely to be calm.

Length constraints per slot, and translatability: what you write has to hold in English and in Arabic too, which means it cannot depend on a sentence''s word order.

Register and language are decided before the first word, not after.', 90),

('design-onboarding-marketing', 'onboarding', 'design', 'marketing', 'en',
 'Onboarding: marketing design',
 'For campaign, social and presentation designers.',
 'One message, several surfaces. The exact formats with their dimensions are part of the brief, and a visual that only works at its original ratio is the pitfall of this family — the crop is where campaigns die.

What has to stay legible at thumbnail size decides the composition, not the other way round.

Platform constraints are real constraints: a format a platform rejects is a deliverable that does not exist.', 100),

('design-onboarding-game', 'onboarding', 'design', 'game', 'en',
 'Onboarding: game UI and environment',
 'For game interface designers and environment or concept artists.',
 'You work inside an engine''s limits: polygon budget, texture size. Those are given before you start, and they are not suggestions.

Legibility **in motion**, at the game''s real speed. An interface designed to be looked at while standing still is the pitfall of this family — it is read during play or it is not read.

Controller as much as mouse. And bring style references as images: a style described in words is a style two people will picture differently.', 110),

('design-onboarding-3d-viz', 'onboarding', 'design', '3d-viz', 'en',
 'Onboarding: architectural and interior visualization',
 'For archviz and interior visualization designers.',
 'Work from the plans or the source model, and match them. A magnificent render whose proportions do not match the drawings is the pitfall of this family, and it is caught by whoever knows the building.

Time of day and light are briefed, not chosen: they decide the mood the client is buying.

Say which degree of realism is wanted — a commercial presentation and a technical study are two different deliverables that look superficially alike.', 120),

('design-onboarding-immersive', 'onboarding', 'design', 'immersive', 'en',
 'Onboarding: AR, VR, spatial and sound',
 'For spatial designers and sound designers.',
 'Comfort is the first criterion, not the last. Which hardware, for how long, and what has been done against nausea. Designing a twenty-minute experience as though it were a two-minute demo is the pitfall of this family, and the cost is paid by the person wearing the headset.

For sound: the listening environment, and how it renders on headphones **and** on a speaker. A mix that only works in one is half a deliverable.', 130),

('design-onboarding-service', 'onboarding', 'design', 'service', 'en',
 'Onboarding: service design and design ops',
 'For service designers and design ops practitioners.',
 'Map the actors, including the ones who never see a screen — they are usually where the process actually breaks.

Touchpoints in order, and what breaks today stated **with facts** rather than with impressions. A journey map that describes the ideal path and not the failing one is the pitfall of this family: the ideal path did not need mapping.

Say what becomes measurable afterwards. A process change nobody can measure is a process change nobody can defend.', 140),

-- ═══════════════════════════════════════════════════════════════════
-- The toolkit
-- ═══════════════════════════════════════════════════════════════════

('design-toolkit', 'toolkit', 'design', NULL, 'en',
 'Design toolkit', 'What to install, by family — and what actually decides it.',
 'Three constraints decide whether a tool is acceptable, and they are the whole requirement:

1. **The delivery format opens for somebody else.** SVG, PDF, PNG, MP4, glTF, WOFF2. An `.ai` on its own is not a deliverable.
2. **Weight.** A 4 GB file is a file the reviewer will not open from a mobile connection.
3. **Resumability.** Sources, structured and named.

**Screens (product, web, mobile).** Penpot is the recommended start: libre, self-hostable, exports SVG, runs on a modest machine. Figma''s free tier runs out of files quickly when you chain challenges. Sketch and Adobe XD are paid alternatives; PhotoPea retouches in the browser.

**Motion and video.** Blender for 3D and rendering, Kdenlive or Shotcut for editing, Rive for interface animation. Deliver interface motion as Lottie — an open, light format, and what a developer will integrate anyway. After Effects and Cinema 4D are the paid equivalents.

**Brand and illustration.** Inkscape and Krita are libre; Illustrator and Affinity Designer are paid. Fonts: Google Fonts, Fontshare and Velvetyne are libre and deliverable.

**Licences — the most expensive trap in the domain.** A desktop-licensed font is not delivered to a client: the licence is yours, not theirs, and they get the invoice. Same rule for photographs, 3D models and component libraries. Either use libre resources, or have the client buy the licence in their own name before you deliver.', 150),

-- ═══════════════════════════════════════════════════════════════════
-- Brief templates, one per family
-- ═══════════════════════════════════════════════════════════════════
--
-- Every design brief carries the same eight sections; each row states them
-- once and then adds what its family needs on top. The eight are not repeated
-- thirteen times — an editor reads the common structure in the first row they
-- open and the additions in the one they are using.

('design-brief-common', 'brief_template', 'design', NULL, 'en',
 'Brief: the eight common sections', 'The structure every design brief carries, whatever the family.',
 '**1. Context.** Who is commissioning, for whom, and what already exists. A designer who does not know what is in place proposes a redesign when a correction was wanted.

**2. Problem.** What is wrong today, **from the point of view of somebody it bothers**. Not the expected solution.
> Bad: "Redo the logo, more modern."
> Good: "The logo is illegible below 24px, and 70% of our supports are favicons and single-colour stamps."

**3. Constraints.** What is not negotiable, and why. Format, support, monochrome, print budget, technical limit, regulation. **An unstated constraint is a wasted iteration.**

**4. Expected deliverables.** The exact list, with formats. "An identity" is not a deliverable; "logotype in SVG, palette with contrast values, a libre or named-licence typeface, and a four-page rules document" is. This section becomes the challenge''s `design_subtype`, and it decides the accepted file size.

**5. Judging criteria.** Taken from the family grid, stated up front. Nobody is judged on a criterion they were not shown.

**6. Accessibility.** Concretely: minimum contrast, minimum body size, an alternative for anything carried by colour alone, captions. It is in the shared grid, so it is asked of every family.

**7. Materials provided.** What the commissioner gives, and under what licence. A brief that provides lorem ipsum produces proposals that break on the real text.

**8. Rounds announced.** How many critique rounds are expected. The hard ceiling is five. Announcing a single round for a brand identity is a promise that will not be kept.', 200),

('design-brief-product', 'brief_template', 'design', 'product', 'en',
 'Brief: product, systems, conversational', 'The eight sections, plus what this family needs.',
 'Add to the common brief:

- **The journey targeted**, from entry point to end.
- **The states**: empty, loading, error, permission refused. A mock-up showing only the nominal case is not usable.
- **Technical targets**: browsers, sizes, dark mode.
- For a system: which products will use it, and who maintains it.

Pitfalls to close in the brief: proposing an isolated screen, forgetting the empty state, drawing a component without saying how it behaves when its text doubles in length.', 210),

('design-brief-web', 'brief_template', 'design', 'web', 'en',
 'Brief: web and editorial', 'The eight sections, plus what this family needs.',
 'Add:

- **The reading hierarchy** expected, and what must be seen first.
- **The real volume of content**, including the worst case — a ninety-character heading.
- **Performance**: weight budget, behaviour with images off.
- For editorial: article length and whether media are present.

Pitfall to close: composing on invented text shorter than the real thing.', 220),

('design-brief-mobile', 'brief_template', 'design', 'mobile', 'en',
 'Brief: mobile', 'The eight sections, plus what this family needs.',
 'Add:

- **The platforms**, and which conventions are respected or knowingly departed from.
- **The thumb zone** and one-handed use.
- **Offline and slow connection** — not negotiable for a West African audience.
- The real sizes targeted, including small screens.

Pitfall to close: mocking up on a 6.7-inch screen only.', 230),

('design-brief-motion', 'brief_template', 'design', 'motion', 'en',
 'Brief: motion and video', 'The eight sections, plus what this family needs.',
 'Add:

- **Duration** and output format.
- **Frame rate** and the platform it plays on.
- **What triggers it**, for interface motion.
- **Sound**: present, or muted by default.
- **Reduced motion**: what happens when it is switched on.

Pitfalls to close: delivering a render without the project; ignoring `prefers-reduced-motion`.', 240),

('design-brief-brand', 'brief_template', 'design', 'brand', 'en',
 'Brief: brand, typography, verbal', 'The eight sections, plus what this family needs.',
 'Add:

- **The real supports** where the brand will appear, from the most constrained to the freest.
- **The worst case of reproduction**: one colour, small, screen-print, embroidery.
- **What already exists**, and what is kept.
- For a typeface: the character set required, languages included, and the weights.

Pitfall to close: presenting a brand only large on a white background.', 250),

('design-brief-illustration', 'brief_template', 'design', 'illustration', 'en',
 'Brief: illustration, icons, characters', 'The eight sections, plus what this family needs.',
 'Add:

- **Render sizes**, and the smallest.
- **Set coherence**: how many pieces, what grid, what stroke weight.
- **Delivery formats** and how files are named.
- For a character: the poses or expressions expected.

Pitfall to close: an icon that is magnificent large and a smudge at 16px.', 260),

('design-brief-dataviz', 'brief_template', 'design', 'dataviz', 'en',
 'Brief: data visualization', 'The eight sections, plus what this family needs.',
 'Add:

- **The real data**, or a representative set — with its outliers.
- **The question** the visualization answers.
- **The audience**: expert or not.
- **Readability without colour**, which is a structural constraint here rather than an addition.

Pitfall to close: a visualization calibrated on clean invented data.', 270),

('design-brief-ux-writing', 'brief_template', 'design', 'ux-writing', 'en',
 'Brief: UX writing', 'The eight sections, plus what this family needs.',
 'Add:

- **Language** and register.
- **Length constraints**, per slot.
- **The error cases** to be written, which are the bulk of the work.
- **Translatability**: what must also hold in English and in Arabic.

Pitfall to close: writing the happy labels and leaving the errors in technical English.', 280),

('design-brief-marketing', 'brief_template', 'design', 'marketing', 'en',
 'Brief: marketing design', 'The eight sections, plus what this family needs.',
 'Add:

- **The exact formats**, with their dimensions.
- **The single message** to carry.
- **The platform constraints** where it will run.
- **What must stay legible** at thumbnail size.

Pitfall to close: a visual that only works at its original ratio.', 290),

('design-brief-game', 'brief_template', 'design', 'game', 'en',
 'Brief: game UI and environment', 'The eight sections, plus what this family needs.',
 'Add:

- **The engine** and its limits: polygon budget, texture size.
- **Legibility in motion**, at the game''s real speed.
- **Controller** as much as mouse.
- **The reference style**, with images.

Pitfall to close: an interface designed to be looked at while standing still.', 300),

('design-brief-3d-viz', 'brief_template', 'design', '3d-viz', 'en',
 'Brief: architectural and interior visualization', 'The eight sections, plus what this family needs.',
 'Add:

- **The plans** or the source model.
- **Time of day and light** expected.
- **The viewpoints** requested.
- **The degree of realism**: commercial presentation or technical study.

Pitfall to close: a magnificent render whose proportions do not match the plans.', 310),

('design-brief-immersive', 'brief_template', 'design', 'immersive', 'en',
 'Brief: AR, VR, spatial and sound', 'The eight sections, plus what this family needs.',
 'Add:

- **The hardware** targeted.
- **The intended session length**, and comfort at that length.
- **Comfort**: what is done against nausea.
- For sound: the listening environment, and rendering on headphones as well as on a speaker.

Pitfall to close: designing a twenty-minute experience as though it were a two-minute demo.', 320),

('design-brief-service', 'brief_template', 'design', 'service', 'en',
 'Brief: service design and design ops', 'The eight sections, plus what this family needs.',
 'Add:

- **The actors**, including those who never see a screen.
- **The touchpoints** in order.
- **What breaks today**, with facts.
- **What becomes measurable** afterwards.

Pitfall to close: a journey map describing the ideal path rather than the failing one.', 330),

-- ═══════════════════════════════════════════════════════════════════
-- Writeup templates
-- ═══════════════════════════════════════════════════════════════════

('design-writeup-version-note', 'writeup_template', 'design', NULL, 'en',
 'Writeup: the version note', 'What accompanies each round you hand in.',
 'Four headings. It is short on purpose — the work is the answer, this says how to read it.

## What I did

## What I changed since the previous round

## What I chose not to change, and why

## What I am unsure about

The third heading is the one people skip and the one reviewers value most: a designer who explains a refusal is arguing, which is the job. A designer who silently ignores a critique is making the next round longer for both of you.

The fourth costs nothing and buys a better critique — naming your own doubt points the reviewer at where you actually want help.', 400),

('design-writeup-critique', 'writeup_template', 'design', NULL, 'en',
 'Writeup: the critique', 'What a reviewer writes back.',
 'A critique concerns a proposal against a brief. It does not concern the person.

## Verdict
`approve`, `iterate` or `reject`, stated first so nobody reads three paragraphs looking for it.

## What I see
Observations, not judgements. "The heading and the body are two sizes apart" rather than "the hierarchy is weak".

## Why that is a problem
Against the brief, or against the family grid. A criticism with no reference is a preference.

## What would change my answer
Concrete and checkable. This is what makes an `iterate` actionable rather than discouraging.

## What is good
Not politeness — information. Somebody who does not know what worked will change it in the next round.', 410),

('design-writeup-case-study', 'writeup_template', 'design', NULL, 'en',
 'Writeup: the case study', 'What you publish once the work is validated.',
 'The document that turns a validated deliverable into something a stranger can read.

## The problem

## The constraints

## What I ruled out
The most-skipped section and the one that shows judgement. Three discarded directions say more about a designer than one finished screen.

## What I delivered

## The critique rounds
Including what came back and what you changed. This is not an admission; it is the part of the story that is actually instructive, and this platform counts work carried to validation after three rounds or more as its own signal.

## What I would do differently', 420);

-- ═══════════════════════════════════════════════════════════════════
-- The one soft_skills trade nobody wrote a guide for
-- ═══════════════════════════════════════════════════════════════════
--
-- Found by the invariant this migration ships with
-- (`no_open_domain_is_left_without_an_onboarding_guide`), on its first run.
-- Design was not the only hole.
--
-- `soft_skills` is the legacy domain. Migration 0500 moved `tech-writer` out
-- of it into `communication` and left `open-source-maintainer` behind, saying
-- in the same breath where that one belongs:
--
--   > `open-source-maintainer` stays in `soft_skills`. […] It belongs to the
--   > leadership split, and moving it here to make a list longer would put a
--   > trade under a review family that cannot judge it.
--
-- So the real fix is a move to `leadership`, on the pattern 0500 established
-- — the row keeps its id, so every `user_orientations` row and every gated
-- slice pointing at it survives. That is a decision about what `leadership`
-- contains, with consequences for its review families and its craft score,
-- and it is not one to take inside a migration about design guides.
--
-- What this row does is narrower and safe: somebody who declares the trade
-- today is served an empty page, and now is not. It says plainly that the
-- trade is legacy, so the guide does not have to be unwritten when the move
-- happens.

INSERT INTO content_guides
    (slug, kind, skill_domain, reviewer_group, locale, title, summary, body_md, sort_order)
VALUES
('soft-skills-open-source-maintainer', 'onboarding', 'soft_skills', NULL, 'en',
 'Onboarding: open source maintainer',
 'The one trade still filed under soft_skills.',
 'Maintaining is triage, review and release management: deciding what a project takes, telling somebody no in a way that keeps them, and shipping on a cadence people can plan against.

The proof here is not commits. It is a queue that moved, a release that went out, and contributors who came back — which is why the deliverables that count are review threads, triage decisions with their reasoning, and releases you cut.

**A note on where this trade lives.** It is filed under `soft_skills`, which is the domain everything predates. Migration 0500 moved `tech-writer` out to `communication` and recorded that this trade belongs with `leadership` instead. Until that move is made, the guidance is here and the review grid that fits best is leadership''s. Nothing about your work changes when it moves — the trade keeps its id, and so does everything you have proved in it.', 10);

COMMENT ON TABLE content_guides IS
    'Onboarding guides, toolkits, brief templates and writeup templates. Rows '
    'rather than files: they have to be translated and edited by somebody who '
    'is not deploying. Design was seeded last (0600) — the documents existed '
    'in docs/design/ from the start and nothing had ported them.';
