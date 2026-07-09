-- Preferred display language for emails, error messages, etc. (Phase 1.13)
-- ISO 639-1 two-letter code. Defaults to NULL → fallback to Accept-Language header,
-- then to "en" (lib fallback).
ALTER TABLE users ADD COLUMN preferred_language CHAR(2);
