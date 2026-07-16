-- Phase BE-E — Audit log append-only + rôle audit_admin.
-- Migration 0099.
--
-- Rationale :
--   Les tables `admin_audit_log` (0014) et `audit_log` (0024) sont écrites
--   par le rôle application `skilluv`. Aujourd'hui rien n'empêche ce rôle
--   de faire `UPDATE` ou `DELETE` sur ces tables — ce qui casserait la
--   chaîne d'audit en cas de compromission du compte app.
--
--   BE-E impose deux invariants :
--
--   1. **Append-only** : le rôle app peut INSERT/SELECT sur audit_log +
--      admin_audit_log, mais PAS UPDATE ni DELETE. Un attaquant qui a le
--      credential app ne peut pas effacer ses traces.
--
--   2. **Rôle audit_admin** : rôle PG dédié à la lecture des logs (SOC / SRE
--      / DPO). NOSUPERUSER, LOGIN, SELECT sur audit tables uniquement.
--
--   La rétention 7 ans est configurable via env `SKILLUV_AUDIT_RETENTION_DAYS`
--   (default 2555 = 7 ans). Un cron worker (à implémenter en BE-E.2 post-MVP)
--   exportera les rows > retention vers S3 avec Object Lock avant DELETE.
--   Actuellement pas de cron actif — les lignes s'accumulent.
--
--   NOTE : `audit_admin` est créé conditionnellement (DO block) car en dev
--   ce rôle n'est pas nécessaire. En prod, un DBA s'assurera qu'il existe
--   avec un password strong.

-- ═══════════════════════════════════════════════════════════════════
-- 1. REVOKE UPDATE/DELETE sur les 2 tables audit
-- ═══════════════════════════════════════════════════════════════════
--
-- Note : PostgreSQL applique les REVOKE au niveau du rôle actuel (celui qui
-- exécute la migration). En dev le rôle `skilluv` est superuser donc les
-- REVOKE sont symboliques (superuser bypass). En prod le rôle app sera
-- NOSUPERUSER et les REVOKE seront effectifs.
--
-- On applique aussi les REVOKE sur `public` pour être défensif contre
-- toute autre role qui aurait été granted par erreur.

-- BE-E : tout le bloc audit_admin en single DO avec advisory lock global.
-- Les GRANT/REVOKE/CREATE ROLE touchent des catalogs globaux PG (pg_authid,
-- pg_database) partagés entre les DBs. Sous tests parallèles avec ~5-10
-- migrations 0099 tournant simultanément sur des DBs différentes, on obtient
-- "tuple concurrently updated". Le lock advisory_xact_lock sérialise tout
-- le bloc à l'échelle du cluster PG (pas de la DB). Libéré à la fin du
-- xact de la migration.
DO $$
BEGIN
    PERFORM pg_advisory_xact_lock(778899001);

    REVOKE UPDATE, DELETE ON admin_audit_log FROM PUBLIC;
    REVOKE UPDATE, DELETE ON audit_log FROM PUBLIC;

    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'skilluv') THEN
        EXECUTE 'REVOKE UPDATE, DELETE ON admin_audit_log FROM skilluv';
        EXECUTE 'REVOKE UPDATE, DELETE ON audit_log FROM skilluv';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'audit_admin') THEN
        BEGIN
            CREATE ROLE audit_admin NOSUPERUSER LOGIN NOINHERIT NOBYPASSRLS;
        EXCEPTION WHEN duplicate_object THEN NULL;
        END;
    END IF;

    EXECUTE 'GRANT CONNECT ON DATABASE ' || quote_ident(current_database()) || ' TO audit_admin';
    EXECUTE 'GRANT USAGE ON SCHEMA public TO audit_admin';
    EXECUTE 'GRANT SELECT ON admin_audit_log TO audit_admin';
    EXECUTE 'GRANT SELECT ON audit_log TO audit_admin';
END $$;

-- ═══════════════════════════════════════════════════════════════════
-- 3. Documentation via COMMENT
-- ═══════════════════════════════════════════════════════════════════

COMMENT ON TABLE admin_audit_log IS
'BE-E — Append-only audit log (legacy admin actions). REVOKE UPDATE/DELETE from app role. Rétention 7 ans configurable via SKILLUV_AUDIT_RETENTION_DAYS. Voir docs/AUDIT-APPEND-ONLY.md.';

COMMENT ON TABLE audit_log IS
'BE-E — Append-only audit log générique (Phase 1.18). REVOKE UPDATE/DELETE from app role. Rétention 7 ans configurable via SKILLUV_AUDIT_RETENTION_DAYS.';

COMMENT ON ROLE audit_admin IS
'BE-E — Rôle read-only pour SOC / SRE / DPO. GRANT SELECT sur audit tables uniquement. En prod : ALTER ROLE audit_admin WITH PASSWORD ''xxx'' + rotation régulière.';
