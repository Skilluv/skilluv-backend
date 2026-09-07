//! P8.4 test suite retired : le dual-write oss_bounties → project_slices est
//! obsolète depuis P9.2 (les bounties SONT des project_slices avec
//! `funder_enterprise_id` NOT NULL, plus de table `oss_bounties`).
//!
//! Le flow bounty est couvert par `test_phase5_bounties.rs` post-P9.2.
