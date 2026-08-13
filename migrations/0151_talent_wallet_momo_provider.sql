-- Store the Mobile Money operator alongside the phone number it belongs to.
--
-- `POST /users/me/wallet/momo/phone` already accepted a `provider` field and
-- threw it away, while `POST /users/me/wallet/withdraw/momo` required one on
-- every call. The front sends `{ amount, currency }` on withdrawal, so every
-- Mobile Money payout failed with `missing field 'provider'`.
--
-- The operator is a property of the number, not of the transaction: a given
-- phone belongs to MTN or to Moov, and that does not change between two
-- withdrawals. Storing it here lets the withdrawal endpoint infer it, and
-- keeps the door open for routing per operator later.
--
-- Nullable on purpose: wallets registered before this migration have a phone
-- and no operator. They keep working — the withdrawal endpoint still accepts
-- an explicit `provider`, and asks for one when it has nothing to fall back
-- on.

ALTER TABLE talent_wallets
    ADD COLUMN momo_provider VARCHAR(20)
        CHECK (momo_provider IS NULL OR momo_provider IN ('orange', 'mtn', 'wave'));

COMMENT ON COLUMN talent_wallets.momo_provider IS
    'Mobile Money operator owning momo_phone. NULL for wallets registered before migration 0151.';
