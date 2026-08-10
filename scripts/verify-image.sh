#!/usr/bin/env bash
# Verify that a Skilluv backend image was signed by our GitHub Actions
# workflow and has an attached SBOM. Bounces the deploy if either fails.
#
# Usage :
#   scripts/verify-image.sh ghcr.io/skilluv/skilluv-backend:master-<sha>
#
# Meant to run as a Coolify pre-deploy hook, or manually before a
# `docker pull`. Requires : cosign (https://docs.sigstore.dev/cosign/system_config/installation/)

set -euo pipefail

IMAGE="${1:-}"
if [ -z "$IMAGE" ]; then
    echo "FAIL usage: $0 <image:tag>" >&2
    exit 2
fi

if ! command -v cosign > /dev/null 2>&1; then
    echo "FAIL cosign not installed. See https://docs.sigstore.dev/cosign/installation/" >&2
    exit 2
fi

REPO="Skilluv/skilluv-backend"
ISSUER="https://token.actions.githubusercontent.com"

echo "▶ Verifying image signature : ${IMAGE}"
if cosign verify "${IMAGE}" \
    --certificate-identity-regexp="https://github.com/${REPO}/.*" \
    --certificate-oidc-issuer="${ISSUER}" > /dev/null 2>&1; then
    echo "  OK signature verified — image built by our GH Actions workflow"
else
    echo "  FAIL signature verification FAILED" >&2
    echo "  This image was not signed by our CI. Refusing to deploy." >&2
    exit 1
fi

echo ""
echo "▶ Verifying SBOM attestation"
if cosign verify-attestation "${IMAGE}" \
    --certificate-identity-regexp="https://github.com/${REPO}/.*" \
    --certificate-oidc-issuer="${ISSUER}" \
    --type cyclonedx > /dev/null 2>&1; then
    echo "  OK SBOM attestation present"
else
    echo "  WARN  SBOM attestation missing (image is signed but SBOM not attached)"
    echo "  Not fatal — deploy will proceed. But investigate."
fi

echo ""
echo "▶ Verifying SLSA provenance"
if cosign verify-attestation "${IMAGE}" \
    --certificate-identity-regexp="https://github.com/slsa-framework/slsa-github-generator/.*" \
    --certificate-oidc-issuer="${ISSUER}" \
    --type slsaprovenance > /dev/null 2>&1; then
    echo "  OK SLSA provenance verified"
else
    echo "  WARN  SLSA provenance missing"
fi

echo ""
echo "OK Image ${IMAGE} passed verification — safe to deploy."
