-- Game skill nodes sit on the "create" axis of the profile wheel.
--
-- 0571 seeded the game skill nodes across several display categories. The
-- platform's convention — asserted by test_p17_2_display_category — is that a
-- whole domain maps to one axis: design and game are "create", security and ops
-- are "operate", and so on. A game node on "understand" or "operate" is a node
-- the profile wheel plots in the wrong slice. Put them all where the domain
-- belongs.

UPDATE skill_nodes SET display_category = 'create'
 WHERE domain = 'game' AND display_category <> 'create';

DO $$
DECLARE n INT;
BEGIN
    SELECT count(*) INTO n FROM skill_nodes
     WHERE domain = 'game' AND display_category <> 'create';
    IF n <> 0 THEN RAISE EXCEPTION '% game nodes are not on the create axis', n; END IF;
END $$;
