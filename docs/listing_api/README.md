# Our-Places-app-api-Rs
gcloud config set run/region us-central1

# from current source path
gcloud run deploy our-places-app-api-rs --source . --allow-unauthenticated --max-instances 1 --memory 256Mi

# Check service
gcloud run services describe our-places-rs

# Generate service.yaml
gcloud run services describe our-places-rs --format export > service.yaml

gcloud beta run services add-iam-policy-binding --region=us-central1 --member=allUsers --role=roles/run.invoker rust-cloud-run

# Same as gcloud run deploy --source .
gcloud builds submit --tag gcr.io/our-places-dev/our_places_rs:dev . --timeout=900

gcloud run deploy our_places_rs --image gcr.io/our-places-dev/our_places_rs:dev --max-instances 1 --memory 256Mi --allow-unauthenticated

# Enable and 
systemctl enable docker
systemctl start docker

sudo systemctl disable docker.service
sudo systemctl disable docker.socket

# Create Service account for OAuth2
gcloud iam service-accounts create our-places-oauth --display-name="Oauth2 Service Account"

gcloud beta run services add-iam-policy-binding ${{ env.SERVICE }} \
    --member ${{ env.SERVICE_ACCOUNT }} \
    --role="roles/run.invoker" \
    --region="us-central1"

# Get service URL
gcloud run services describe ${{ env.SERVICE }} --region us-central1 --format='value(status.url)'

# Dev->Run Flow
**Clean and Run Tests**
> docker stop $(docker ps -a -q); docker rm $(docker ps -a -q --filter ancestor=ourplaces-db-image)
> cargo make docker-db-run
> SKIP_DOCKER=true cargo make run-db-migrations
> cargo test
