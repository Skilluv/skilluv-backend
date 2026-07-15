-- Phase P24.2 — Table agency_clients pour workflow staffing_agency.
-- Migration 0096.
--
-- Rationale :
--   Une agence de recrutement (`enterprises.enterprise_type = 'staffing_agency'`)
--   recrute pour des clients tiers. Elle a besoin de gérer un carnet de
--   clients : contact, notes, statut actif/inactif. Chaque shortlist / contact
--   avec un talent peut être attribuée à un client final (pour l'invoicing).
--
--   Contrainte métier : seule une agence peut créer des rows dans
--   `agency_clients`. Enforcée via CHECK + trigger.

CREATE TABLE agency_clients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    enterprise_id UUID NOT NULL REFERENCES enterprises(id) ON DELETE CASCADE,
    client_name VARCHAR(200) NOT NULL
        CHECK (length(client_name) BETWEEN 2 AND 200),
    client_contact_email VARCHAR(255),
    notes TEXT,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (enterprise_id, client_name)
);

CREATE INDEX idx_agency_clients_by_enterprise
    ON agency_clients (enterprise_id, active DESC, created_at DESC);

-- Trigger : refuse l'insertion si l'enterprise n'est pas de type staffing_agency.
CREATE OR REPLACE FUNCTION check_agency_client_enterprise_type()
RETURNS TRIGGER AS $$
DECLARE
    ent_type VARCHAR(30);
BEGIN
    SELECT enterprise_type INTO ent_type
    FROM enterprises WHERE id = NEW.enterprise_id;
    IF ent_type IS DISTINCT FROM 'staffing_agency' THEN
        RAISE EXCEPTION
            'agency_clients can only be added to enterprises with enterprise_type = ''staffing_agency'' (got ''%'')',
            COALESCE(ent_type, 'null');
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agency_clients_enforce_type
    BEFORE INSERT OR UPDATE OF enterprise_id ON agency_clients
    FOR EACH ROW EXECUTE FUNCTION check_agency_client_enterprise_type();
