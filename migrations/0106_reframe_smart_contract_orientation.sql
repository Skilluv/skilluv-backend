-- Content strategy foundation — reframe orientation smart-contract-dev.
--
-- Rationale décision 6 de la stratégie contenu 2027-2028 :
--   Skilluv choisit d'assumer la blockchain SANS la crypto trading / DeFi
--   spéculatif. L'orientation smart-contract-dev existe déjà (migration 0088)
--   mais sa description ("Solidity, Cairo, contrats on-chain, sécurité DeFi.")
--   pointe vers l'usage spéculatif que la ligne éditoriale rejette.
--
--   Ré-écriture pour dégager les usages d'utilité réelle, avec ancrage africain :
--   - Identité souveraine / DIDs (pour populations sans état civil fiable)
--   - Traçabilité produits agricoles (chaîne cacao, café, coton)
--   - Registres fonciers décentralisés (problème brûlant en Afrique)
--   - Monnaies communautaires locales (Sarafu au Kenya = success story)
--   - Escrow trustless pour bounties Skilluv eux-mêmes
--   - Attestations diplômes vérifiables on-chain (option future)
--
--   Rôle éducatif Skilluv : "blockchain ≠ scam crypto". Décision utilisateur.
--
-- Ajout Rust (Solana, ink!) pour cohérence avec la ligne Rust de la plateforme.

UPDATE orientations
SET description = 'Solidity (Ethereum), Cairo (Starknet), Rust (Solana, ink!). Contrats intelligents pour l''utilité réelle : identité souveraine (DIDs), traçabilité produits agricoles, registres fonciers décentralisés, monnaies communautaires, escrow trustless. Focus impact africain, pas spéculation crypto.',
    tags = ARRAY['blockchain', 'web3', 'africa-impact']::TEXT[],
    updated_at = NOW()
WHERE slug = 'smart-contract-dev';
