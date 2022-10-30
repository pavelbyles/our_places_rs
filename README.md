# Our-Places-Rs

gcloud config set run/region us-central1

# from current source path
gcloud run deploy --source .

# Check service
gcloud run services describe our-places-rs

gcloud beta run services add-iam-policy-binding --region=us-central1 --member=allUsers --role=roles/run.invoker rust-cloud-run

gcloud builds submit --tag gcr.io/our-places-dev/our_places_rs:dev . --timeout=900

gcloud run deploy our_places_rs --image gcr.io/our-places-dev/our_places_rs:dev --max-instances 1 --memory 256Mi --allow-unauthenticated