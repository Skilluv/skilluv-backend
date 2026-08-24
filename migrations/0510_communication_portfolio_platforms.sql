-- Where a communicator's recorded career already lives.
--
-- Rows in `portfolio_platforms` (migration 0415), which turned fourteen
-- platform values in a CHECK into a table and carried what each one's numbers
-- mean — items are repositories on GitHub, tracks on Bandcamp, and articles
-- here.
--
-- ## Tickets P-01 and P-02, and the one thing they asked for that is not here
--
-- Both asked for scraping: reading a public Medium page, or an Apple Podcasts
-- listing, to obtain figures the platform does not publish. 0415 already
-- refused that and gave two reasons that hold exactly: the terms of those
-- services forbid it, and a figure obtained that way is indistinguishable —
-- in this table — from one somebody typed.
--
-- So the platforms with no API are linked, their figures are accepted as
-- declared, `verified_at` stays NULL, and `communication_profile` counts them
-- at half weight. That is the same treatment audio gets, and this domain
-- needs it more: the single most common home for a technical blog is a
-- personal domain nobody has an API for.
--
-- ## `personal_blog` has an API and it is called RSS
--
-- The exception worth naming. A self-hosted blog publishes a feed, the feed
-- lists the posts, and counting them is a fetch rather than a claim. No feed
-- publishes readership, so `reach_label` is NULL: the item count is checked
-- and the audience is simply absent, which is the honest pair.
--
-- ## Why podcasts are two rows and not one
--
-- Spotify and Apple host the same episodes and answer differently: Spotify
-- has a creator API nobody outside the show can use, Apple has none at all.
-- One row would have to pick one of those two answers for both.

INSERT INTO portfolio_platforms
    (slug, skill_domain, name, profile_url_pattern, items_label, reach_label, has_public_api, sort_order) VALUES
    ('dev_to', 'communication', 'DEV',
     'https://dev.to/{handle}', 'articles', 'reactions', TRUE, 310),
    ('hashnode', 'communication', 'Hashnode',
     'https://hashnode.com/@{handle}', 'articles', 'views', TRUE, 320),
    ('medium', 'communication', 'Medium',
     'https://medium.com/@{handle}', 'articles', 'claps', FALSE, 330),
    ('personal_blog', 'communication', 'Personal blog',
     NULL, 'articles', NULL, TRUE, 340),
    ('youtube', 'communication', 'YouTube',
     'https://www.youtube.com/@{handle}', 'videos', 'views', TRUE, 350),
    ('twitch', 'communication', 'Twitch',
     'https://www.twitch.tv/{handle}', 'streams', 'viewers', TRUE, 360),
    ('spotify_podcast', 'communication', 'Spotify (podcast)',
     NULL, 'episodes', NULL, FALSE, 370),
    ('apple_podcast', 'communication', 'Apple Podcasts',
     NULL, 'episodes', NULL, FALSE, 380),
    ('weblate', 'communication', 'Weblate',
     'https://hosted.weblate.org/user/{handle}/', 'translations', NULL, TRUE, 390),
    ('crowdin', 'communication', 'Crowdin',
     'https://crowdin.com/profile/{handle}', 'translations', NULL, FALSE, 400);
