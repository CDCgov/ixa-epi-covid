set -euo pipefail

# Load .env if it exists
if [[ -f .env ]]; then
  set -a
  source .env
  set +a
fi

: "${AZURE_CONTAINER_REGISTRY_ACCOUNT:?Set AZURE_CONTAINER_REGISTRY_ACCOUNT in .env}"

ACR_DOMAIN="${AZURE_CONTAINER_REGISTRY_DOMAIN:-azurecr.io}"
ACR_SERVER="${AZURE_CONTAINER_REGISTRY_ACCOUNT}.${ACR_DOMAIN}"
IMAGE="${ACR_SERVER}/ixa-epi-covid:latest"

# Obtain a short-lived ACR token using your Azure CLI login
ACR_TOKEN="$(
  az acr login \
    --name "$AZURE_CONTAINER_REGISTRY_ACCOUNT" \
    --expose-token \
    --query accessToken \
    --output tsv
)"

# Authenticate Docker to ACR
printf '%s' "$ACR_TOKEN" |
  docker login "$ACR_SERVER" \
    --username 00000000-0000-0000-0000-000000000000 \
    --password-stdin

# Build and push the image
docker build \
  -t "$IMAGE" \
  .

docker push "$IMAGE"