-- The code entrance stops being a way to earn the right to judge others.
--
-- ## What changed above this
--
-- Migration 0624 gave the reviewer ladder a first rung. The circle it broke
-- was real: a rite verdict wants `admin`, `mentor` or `domain_curator`; only
-- `mentor` is automatic; it wants five attestations or three mentorship
-- sessions; both need work somebody already validated; validating needs one
-- of those capabilities. No reviewer, no completed rite, no attestation, no
-- mentor, no reviewer.
--
-- `rite_reviewer:{domain}` broke it the way a compagnonnage does: whoever
-- passed the entrance of a trade may witness the next entrance of that trade.
-- Person one needs a steward; person two can be read by person one. That
-- reasoning is untouched and the rung stays for the eleven domains whose
-- entrance still ends in a person reading it.
--
-- ## Why code is no longer one of them
--
-- The code entrance is decided by `services::hello_check`, not by a reviewer.
-- Its claim - "I can fork a repository, edit a file, open a pull request" -
-- is mechanically true or false, so a person opening that diff was
-- acknowledging receipt rather than exercising judgement, and charging
-- somebody a wait for an acknowledgement is the wrong first impression for a
-- platform whose subject is doing.
--
-- But a capability to witness others, granted by a process in which nobody
-- witnessed anything, anchors the chain of trust to nothing. Passing an
-- automated check cannot be what earns the right to judge a person. So the
-- code rung goes with the code verdict: both leave together, or neither
-- should have.
--
-- ## Why the rows are revoked and not deleted
--
-- `user_capabilities` is an audit trail - `granted_at`, `granted_by`,
-- `revoked_at` - and somebody who held this yesterday did. Deleting the row
-- would make the record say they never had it, which is a different and false
-- claim. Revoking says what happened: they held it, and the rule changed.
--
-- The application refuses it in two places as well (`capabilities_engine`
-- stops granting, `services::reviews` stops honouring), because a capability
-- already held has to stop working rather than merely stop being issued -
-- this migration closes the door, and those two keep it closed.

UPDATE user_capabilities
   SET revoked_at = NOW(),
       revoked_reason = 'the code entrance is decided automatically; passing it no longer confers the right to review it'
 WHERE capability = 'rite_reviewer:code'
   AND revoked_at IS NULL;
