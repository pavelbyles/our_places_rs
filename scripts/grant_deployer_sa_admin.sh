#!/usr/bin/env bash
# scripts/grant_deployer_sa_admin.sh — Let the CI deployer manage IAM on the runtime service accounts
#
# CI grants roles/iam.serviceAccountTokenCreator on these service accounts
# (listing_api signs upload URLs as itself; the Pub/Sub agent mints push
# tokens for the email worker). Editing a service account's IAM policy needs
# roles/iam.serviceAccountAdmin, so this grants it to the deployer on just
# these accounts, not project-wide. Run once per project as an owner.
set -euo pipefail

PROJECT_ID="${1:-our-places-dev}"
DEPLOYER="${DEPLOYER:-sa-our-places-oauth}"
TARGETS=(
  sa-listing-api
  sa-listing-api-prod
  sa-email-worker
  sa-email-worker-prod
)

DEPLOYER_EMAIL="$DEPLOYER@$PROJECT_ID.iam.gserviceaccount.com"

for sa in "${TARGETS[@]}"; do
  sa_email="$sa@$PROJECT_ID.iam.gserviceaccount.com"

  if ! gcloud iam service-accounts describe "$sa_email" --project="$PROJECT_ID" >/dev/null 2>&1; then
    echo "SKIP  $sa_email (does not exist)"
    continue
  fi

  echo "GRANT roles/iam.serviceAccountAdmin on $sa_email to $DEPLOYER_EMAIL"
  gcloud iam service-accounts add-iam-policy-binding "$sa_email" \
    --project="$PROJECT_ID" \
    --member="serviceAccount:$DEPLOYER_EMAIL" \
    --role="roles/iam.serviceAccountAdmin" \
    --condition=None \
    --quiet >/dev/null
done

echo "Done. Re-run the failed GitHub Actions job."
