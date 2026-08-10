# Payment flows — staging setup & end-to-end test (SKI-35)

Guide pour créer les comptes staging Stripe Connect + Mobile Money, puis
runner les scripts `scripts/test-payment-stripe.sh` / `test-payment-momo.sh`
qui font un vrai payout aller-retour de faible montant.

## Prérequis

- Compte GitHub Skilluv (pour lire ce doc)
- Compte email pour créer les accounts test
- 1€ + 1000 XOF de "budget" (rappel : les APIs test/sandbox ne débitent PAS de vraies cartes/comptes)

---

## 1. Stripe Connect test account

### 1.1. Créer le compte Stripe test

1. https://dashboard.stripe.com/register — si pas déjà fait
2. En haut à droite : bascule sur "Test mode" (le toggle Test → Live). Tout ce qui suit est en test.
3. Menu → Developers → API keys → copier **Secret key** commence par `sk_test_...`

### 1.2. Activer Connect en test

1. https://dashboard.stripe.com/test/connect/accounts → **Get started with Connect** si première fois
2. Choisir "Standard" (les talents Skilluv onboardent leur propre compte Stripe)
3. Copier le `client_id` (starts with `ca_test_...`)

### 1.3. Créer un compte Connect test (le "talent" fictif)

Depuis le dashboard :
1. Test mode → Connect → Accounts → **Add account**
2. Type : Standard
3. Country : France (ou pays Africain supporté par Stripe)
4. Créer le compte, copier l'`account_id` (starts with `acct_test_...`)

Stripe fournit des **test card numbers** qui simulent tous les scénarios : succès (4242 4242 4242 4242), decline (4000 0000 0000 0002), 3DS challenge (4000 0000 0000 3220). Voir https://stripe.com/docs/testing.

### 1.4. Env vars pour le script

```bash
export STRIPE_TEST_SECRET_KEY="sk_test_..."
export STRIPE_TEST_CONNECT_ACCOUNT="acct_test_..."
export SKILLUV_BASE_URL="https://staging.skill-uv.com"  # ou http://localhost:3001
```

### 1.5. Lancer le test

```bash
./scripts/test-payment-stripe.sh
```

Ce que le script fait :
1. Login sur Skilluv avec un compte test (créer si besoin)
2. POST `/api/talent-wallet/payouts` avec montant 100 (1€)
3. Vérifie la réponse : `{status: "pending", stripe_transfer_id: "tr_test_..."}`
4. Vérifie que la transfer arrive sur le compte Connect test (via Stripe API `transfers.retrieve`)
5. Simule le webhook `transfer.paid` reçu par le back
6. Vérifie que le back a bien marqué la transaction en `paid` dans `talent_wallet_transactions`

**Cleanup** : le script tag les rows créées avec `test_run_id` pour cleanup manuel via un `DELETE FROM talent_wallet_transactions WHERE test_run_id = 'xxx'` post-run.

---

## 2. Mobile Money sandbox

Skilluv supporte 3 providers : Orange Money, MTN Mobile Money, Wave. Chacun a son sandbox.

### 2.1. Orange Money sandbox

1. https://developer.orange.com/apis/om-webpay/getting-started → sign up
2. Créer une app "Skilluv-Staging" → obtenir `client_id` + `client_secret`
3. Numéro sandbox : Orange fournit des numéros test qui simulent tous les codes retour (success, insufficient funds, timeout)
4. Env vars :
   ```bash
   export ORANGE_MONEY_CLIENT_ID="..."
   export ORANGE_MONEY_CLIENT_SECRET="..."
   export ORANGE_MONEY_SANDBOX_MSISDN="+22890000001"  # numéro test fourni
   ```

### 2.2. MTN Mobile Money sandbox

1. https://momodeveloper.mtn.com/ → sign up
2. Subscribe to "Collections" + "Disbursements" products (both free tier)
3. Obtenir `API_USER` (UUID) + `API_KEY` via l'UI (Subscription key visible en haut)
4. Env vars :
   ```bash
   export MOMO_SANDBOX_API_USER="uuid-..."
   export MOMO_SANDBOX_API_KEY="..."
   export MOMO_SANDBOX_SUBSCRIPTION_KEY="..."
   export MOMO_SANDBOX_MSISDN="46733123454"  # numéro sandbox MTN
   ```

### 2.3. Wave sandbox

1. https://docs.wave.com/business/getting-started → account manager (contact commercial)
2. Sandbox key fourni par email après signup

### 2.4. Lancer le test Momo

```bash
export SKILLUV_MOMO_PROVIDER="mtn"  # ou "orange" ou "wave"
./scripts/test-payment-momo.sh
```

Ce que le script fait :
1. Login sur Skilluv
2. POST `/api/talent-wallet/payouts` avec `{amount: 1000, currency: "XOF", provider: "mtn", msisdn: "..."}`
3. Poll `/api/talent-wallet/transactions/{id}` toutes les 5s jusqu'à `status=succeeded` ou `failed` (max 60s)
4. Vérifie la row dans `talent_wallet_transactions`

---

## 3. Cas d'échec à tester

Une fois les happy paths OK, vérifier :

### Webhook Stripe perdu
- Fais un payout, puis simule "le webhook n'arrive jamais"
- Le back doit avoir un poller qui rattrape (chercher `services::stripe::start_reconciliation_task` si existe)
- Après ~5 min, la transaction doit être en `succeeded` quand même

### Momo timeout
- Numéro sandbox avec code `TIMEOUT` (Orange fournit `+22890099999`)
- La transaction doit finir en `failed` avec `error_code = "TIMEOUT"`
- Le talent_wallet doit avoir été crédité back (rollback du hold)

### Insufficient funds sandbox
- Numéro sandbox `INSUFFICIENT_FUNDS`
- Idem : `failed` propre + rollback

## 4. Runbook incident

Voir `docs/PAYMENT_INCIDENT_RESPONSE.md` (à créer post-run — dépend des cas rencontrés).

En résumé pour un vrai incident prod :
1. Chercher la transaction dans `talent_wallet_transactions` par `stripe_transfer_id` ou `momo_reference_id`
2. Vérifier le status côté provider (Stripe dashboard / Momo dashboard)
3. Si mismatch : rejouer le webhook manuellement via un endpoint admin (à ajouter en follow-up si besoin) OU update manuel la row + notif user

---

## 5. Checklist avant "go" prod payments

- [ ] Stripe Connect test payout 1€ reçu sur compte test
- [ ] Momo sandbox 1000 XOF reçu sur numéro test
- [ ] 3 cas d'échec testés (webhook perdu, timeout, insufficient funds)
- [ ] Runbook incident prêt
- [ ] Alertes ops (SKI-32) câblées sur payment webhook failure
- [ ] Bascule des env vars TEST → LIVE côté prod Coolify
- [ ] Premier vrai payout à un vrai talent Skilluv : montant minimal (~5€), vérifier réception, feedback
