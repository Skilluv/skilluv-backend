-- The link between an event and the stamp it awards.
--
-- ## What was missing
--
-- Migration 0093 wrote the plan down: the stamp is awarded by `badge_engine`
-- consuming `user_event_participation` as proof of an
-- `output_type = 'event_stamp'`, and "le stamp reste à vie sur le profil".
--
-- The table shipped. The badge category shipped, in 0090. The proof type never
-- did — `badge_engine` has no `event_participated` — so **no event stamp has
-- ever been awarded**, and nothing connected an event to the badge it was
-- supposed to produce.
--
-- The front noticed from the other end: its `/users/me/events` client declared
-- a `stamp_earned` field that no endpoint had ever served, and rendered a
-- "Timbre gagné" badge on a value that was permanently `undefined` (SKI-352).
-- It now shows "contribution comptée" instead, which was the right call —
-- "something was counted" is not "a stamp was issued", and only the first was
-- true.
--
-- ## This column
--
-- Which badge this event's stamp is. Nullable, and most events will leave it
-- null: a community meetup awards nothing, and that is not a gap. A hackathon
-- that does award one names it here, and `/users/me/events` can then answer
-- `stamp_earned` truthfully instead of guessing.
--
-- Not a foreign key to `badges(slug)` on purpose: an event is usually
-- announced before its badge is authored, and refusing to create the event
-- until the badge exists would put the schema in charge of an editorial
-- sequence. The join tolerates a slug with no badge behind it — it reads as
-- "not earned", which is what it is.

ALTER TABLE events
    ADD COLUMN stamp_badge_slug VARCHAR(60);

COMMENT ON COLUMN events.stamp_badge_slug IS
    'Slug of the badge this event awards as its stamp, or NULL when it awards '
    'none. Read by /users/me/events to answer `stamp_earned`. Deliberately not '
    'a foreign key: an event is announced before its badge is written.';

-- Only where it is set, which is the minority.
CREATE INDEX idx_events_stamp_badge ON events (stamp_badge_slug)
    WHERE stamp_badge_slug IS NOT NULL;
