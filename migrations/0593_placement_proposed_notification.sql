-- A placement proposal reaches the person it concerns.
--
-- propose_placement inserts the row and nothing leaves for the junior, so an
-- enterprise could offer a two-year placement — a salary, a duration, a
-- guarantee — and the person had no way to learn it existed (SKI-331). This
-- registers the notification kind so the propose route can tell them, the same
-- way reverse_recruitment.pitch_received tells a talent a company argued for
-- itself.

INSERT INTO notification_kinds
    (kind, category, allows_in_app, allows_push, allows_email,
     default_in_app, default_push, default_email, transactional)
VALUES
    -- To the junior, when a company proposes a long placement. Every channel,
    -- default on: this is long-term hiring, not a contest invite, and it is
    -- exactly the kind of thing a person must not miss. Not transactional —
    -- there is no hard clock — but too consequential to be opt-in.
    ('placement.proposed', 'enterprise',
     TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, FALSE)
ON CONFLICT (kind) DO NOTHING;
