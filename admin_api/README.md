# Our-Places-app-api-Rs
gcloud config set run/region us-central1

# from current source path
gcloud run deploy our-places-app-admin-api-rs --source . --allow-unauthenicated --max-instances 1 --memory 256Mi