-- A wall tile is not a source file.
--
-- `design_upload_sessions` ceilings are for the thing itself: 100 MB for a
-- copy deck, 200 MB for a brand kit, 500 MB for an interface file or a sound
-- piece. A wall is a grid of two dozen of those at once, and most of the
-- people it is built for are on a phone paying for the data.
--
-- Two ways to get a small image out of a large file, and only one of them is
-- honest here.
--
-- Generating it would mean decoding somebody else's file in our process. That
-- is a decompression bomb waiting to be sent, and it needs an image library
-- whose dependency tree `cargo vet` would have to be walked crate by crate
-- for a thumbnail. It also does badly at the only part that matters: which
-- frame, which crop, which ten seconds. A motion designer answers that better
-- than a resizer does.
--
-- So the author supplies it, exactly as they already supply the preview for
-- the four subtypes a browser cannot open. `cover_key` is that, for every
-- subtype, and it reuses the preview mechanism unchanged: a separate
-- object with a separate lifetime, replaceable without re-uploading five
-- gigabytes.
--
-- Nullable, because it is not a wall entry that is at stake if it is missing.
-- `services::hello_wall` falls back to the preview, then to the file itself
-- and only while the file is small enough to be a tile. Past that it shows
-- the entry with its text and no picture, which is a worse tile and a better
-- deal than making somebody download a 200 MB brand kit to see a face.

ALTER TABLE design_upload_sessions
    ADD COLUMN cover_key TEXT;

COMMENT ON COLUMN design_upload_sessions.cover_key IS
    'Where the author supplied still image lands, if they supplied one. What a grid shows: the source file is sized for the work, not for a tile. Distinct from preview_key, which exists so a reviewer can open the four subtypes a browser cannot.';
