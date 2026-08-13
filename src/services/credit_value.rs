//! What one credit is worth.
//!
//! Enterprises buy credits at a published price and spend them on bounties;
//! the talent who completes one is paid in real money. Somewhere, credits
//! have to become currency.
//!
//! That conversion used to read a rate from `BOUNTY_CREDIT_TO_EUR` /
//! `BOUNTY_CREDIT_TO_XOF`, defaulting to `0.0`. Two problems, one of them
//! serious:
//!
//!   * Unset meant zero, so the payout silently did not happen while the
//!     slice was still stamped as paid.
//!   * A rate in an environment variable, changed by hand, is foreign
//!     exchange — a regulated activity, and not one we perform.
//!
//! Neither is what is actually going on. A credit is prepaid at a fixed
//! published price, so converting it back is not an exchange: it is the
//! denomination the enterprise already paid in. The value belongs next to
//! the pack definitions, in code, reviewable, with no runtime knob that can
//! be wrong.
//!
//! When Skilluv genuinely needs to convert between currencies — paying a
//! euro balance out in francs — that is done by the payout provider at the
//! rate of the day, not here. See `services::payout`.

use bigdecimal::BigDecimal;

use crate::services::ledger::Currency;

/// Euro value of one credit.
///
/// Matches the pack pricing in `services::stripe::PACKS`. Changing this
/// changes what every unspent credit is worth, so it changes with the
/// published price and not otherwise.
pub const CREDIT_VALUE_EUR: i64 = 1;

/// Franc CFA value of one credit.
///
/// The XOF is pegged to the euro at 655.957 francs, fixed by the monetary
/// agreement rather than by a market, which is why a constant is honest here
/// and would not be for a floating currency. Rounded to the franc: the XOF
/// has no subdivision, and a fractional amount would be truncated by the
/// provider anyway.
pub const CREDIT_VALUE_XOF: i64 = 656;

/// Fragments awarded per credit of bounty reward.
///
/// Gamification, not money — fragments buy nothing and cannot be withdrawn.
/// It lived in `BOUNTY_CREDIT_TO_FRAGMENTS` with a default of 500, which was
/// less dangerous than the currency rate (an unset variable still awarded
/// something) but wrong for the same reason: how generous the platform is
/// with its own points is a product decision, reviewed like one, not a knob
/// an operator can turn.
pub const FRAGMENTS_PER_CREDIT: i64 = 500;

/// Convert an amount of credits into currency.
pub fn to_currency(credits: &BigDecimal, currency: Currency) -> BigDecimal {
    let rate = match currency {
        Currency::Eur => CREDIT_VALUE_EUR,
        Currency::Xof => CREDIT_VALUE_XOF,
    };
    let converted = credits * BigDecimal::from(rate);
    match currency {
        // No minor unit: whole francs only.
        Currency::Xof => converted.with_scale(0),
        // Two decimals, rounding half away from zero — the same rule a bank
        // statement uses.
        Currency::Eur => converted.round(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn dec(s: &str) -> BigDecimal {
        BigDecimal::from_str(s).unwrap()
    }

    #[test]
    fn credits_convert_to_euros_at_the_published_value() {
        assert_eq!(to_currency(&dec("100"), Currency::Eur), dec("100"));
        assert_eq!(to_currency(&dec("42.5"), Currency::Eur), dec("42.50"));
    }

    #[test]
    fn credits_convert_to_whole_francs() {
        assert_eq!(to_currency(&dec("100"), Currency::Xof), dec("65600"));
        // A fractional franc cannot be paid out, so it is not produced.
        let converted = to_currency(&dec("0.5"), Currency::Xof);
        assert_eq!(converted.fractional_digit_count().max(0), 0);
    }

    #[test]
    fn zero_credits_convert_to_zero() {
        // The old code reached this by accident, through an unset variable,
        // and treated it as a successful payout.
        assert_eq!(to_currency(&dec("0"), Currency::Eur), dec("0"));
        assert_eq!(to_currency(&dec("0"), Currency::Xof), dec("0"));
    }

    #[test]
    fn conversion_never_silently_yields_nothing_for_a_real_amount() {
        // The regression that mattered: any positive number of credits must
        // produce a positive amount, in every currency.
        for credits in ["0.01", "1", "7.5", "1000"] {
            for currency in [Currency::Eur, Currency::Xof] {
                let out = to_currency(&dec(credits), currency);
                assert!(
                    out > dec("0"),
                    "{credits} credits produced {out} in {}",
                    currency.as_str()
                );
            }
        }
    }
}
