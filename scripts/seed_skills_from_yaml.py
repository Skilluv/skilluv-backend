#!/usr/bin/env python3
"""
Génère la migration 0057 (seed skill_nodes) depuis docs/skill-nodes-seed.yaml.

Usage :
    python scripts/seed_skills_from_yaml.py

Sortie :
    migrations/0057_seed_skill_nodes.sql

Le script est idempotent : re-run overwrite le fichier de migration. À re-lancer
quand le seed YAML est modifié (ajout/retrait/renommage de skills).

Le seed YAML est le SEUL SOURCE OF TRUTH pour les skills atomiques Skilluv.
Ne PAS éditer le SQL généré directement — il sera écrasé.
"""

import sys
from pathlib import Path

try:
    import yaml
except ImportError:
    print("Missing pyyaml. Install with: python -m pip install pyyaml", file=sys.stderr)
    sys.exit(1)


ROOT = Path(__file__).parent.parent
YAML_PATH = ROOT / "docs" / "skill-nodes-seed.yaml"
OUT_PATH = ROOT / "migrations" / "0057_seed_skill_nodes.sql"


def escape_sql(text: str | None) -> str:
    """Escape single quotes for SQL string literal. Returns NULL if input is None."""
    if text is None:
        return "NULL"
    return "'" + text.replace("'", "''") + "'"


def generate_sql() -> str:
    """Parse YAML and produce SQL INSERT statements for skill_nodes."""
    with open(YAML_PATH, encoding="utf-8") as f:
        data = yaml.safe_load(f)

    if not isinstance(data, list):
        raise ValueError("Root of YAML must be a list of domain entries")

    category_inserts: list[str] = []
    skill_inserts: list[str] = []
    skill_count = 0
    category_count = 0

    for domain_entry in data:
        domain = domain_entry["domain"]
        for category in domain_entry.get("categories", []):
            cat_slug = category["slug"]
            cat_display = category["display"]
            cat_skilluv = category.get("skilluv_specific", False)
            category_count += 1

            category_inserts.append(
                f"    ({escape_sql(cat_slug)}, "
                f"{escape_sql(cat_display)}, "
                f"{escape_sql(domain)}, "
                f"NULL, "
                f"{'TRUE' if cat_skilluv else 'FALSE'})"
            )

            for skill in category.get("skills", []):
                skill_slug = skill["slug"]
                skill_display = skill["display"]
                skill_desc = skill.get("description")
                skill_skilluv = skill.get("skilluv_specific", False) or cat_skilluv
                skill_count += 1

                skill_inserts.append(
                    f"    ({escape_sql(skill_slug)}, "
                    f"{escape_sql(skill_display)}, "
                    f"{escape_sql(skill_desc)}, "
                    f"{escape_sql(domain)}, "
                    f"{escape_sql(cat_slug)}, "
                    f"{'TRUE' if skill_skilluv else 'FALSE'})"
                )

    sql = f"""-- Phase P0 — Fondations du modèle cible.
-- Migration 0057 : seed initial de la table `skill_nodes`.
--
-- Ce fichier est AUTO-GÉNÉRÉ depuis docs/skill-nodes-seed.yaml
-- Ne pas éditer à la main. Re-générer avec :
--     python scripts/seed_skills_from_yaml.py
--
-- Contient :
--     {category_count} catégories (parent_id = NULL)
--     {skill_count} skills atomiques (parent_id = FK vers leur catégorie)
--
-- Total : {category_count + skill_count} lignes insérées dans skill_nodes.
--
-- L'ordre d'insertion est important : catégories d'abord (elles servent de
-- parent aux skills), puis skills atomiques dont le parent_id est résolu
-- par sous-requête via `slug`.

-- ═══════════════════════════════════════════════════════════════════
-- Étape 1 : Insérer les catégories (parent_id = NULL)
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, domain, parent_id, is_skilluv_specific)
VALUES
{",\n".join(category_inserts)}
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Étape 2 : Insérer les skills atomiques avec parent_id résolu par slug
-- ═══════════════════════════════════════════════════════════════════

WITH new_skills (slug, display_name, description, domain, parent_slug, is_skilluv_specific) AS (
    VALUES
{",\n".join(skill_inserts)}
)
INSERT INTO skill_nodes (slug, display_name, description, domain, parent_id, is_skilluv_specific)
SELECT
    ns.slug,
    ns.display_name,
    ns.description,
    ns.domain,
    parent.id,
    ns.is_skilluv_specific
FROM new_skills ns
JOIN skill_nodes parent ON parent.slug = ns.parent_slug AND parent.parent_id IS NULL
ON CONFLICT (slug) DO NOTHING;

-- ═══════════════════════════════════════════════════════════════════
-- Vérification (commentaires — les COUNT ne s'exécutent pas dans une migration)
-- ═══════════════════════════════════════════════════════════════════
--
-- Après application, on doit avoir :
--     SELECT COUNT(*) FROM skill_nodes WHERE parent_id IS NULL;     -- {category_count} catégories
--     SELECT COUNT(*) FROM skill_nodes WHERE parent_id IS NOT NULL; -- {skill_count} skills atomiques
--     SELECT COUNT(*) FROM skill_nodes;                             -- {category_count + skill_count} total
"""
    return sql


def main() -> None:
    if not YAML_PATH.exists():
        print(f"YAML not found: {YAML_PATH}", file=sys.stderr)
        sys.exit(1)

    sql = generate_sql()
    OUT_PATH.write_text(sql, encoding="utf-8")

    # Basic stats
    line_count = sql.count("\n")
    insert_count = sql.count("INSERT INTO skill_nodes")
    print(f"Generated {OUT_PATH}")
    print(f"  {line_count} lines")
    print(f"  {insert_count} INSERT statements")


if __name__ == "__main__":
    main()
