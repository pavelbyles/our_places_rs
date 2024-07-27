#!/bin/bash

# Project Id of the GCP project you'll be using this with
export PROJECT_ID=our-places-dev
# What you want to name the service (e.g. cloud-run-flask-oauth2)
export SERVICE_NAME=our-places-app-api-rs
# Relative path of your service account credentials file
export CREDS=our-places-dev-sa-our-places-oauth.json
# Service account email address that will have access to this resource
export SERVICE_ACCOUNT_EMAIL=sa-our-places-oauth@our-places-dev.iam.gserviceaccount.com
export CLOUD_RUN_SERVICE_URL=$(eval "gcloud run services describe $SERVICE_NAME --region us-central1 --format='value(status.url)'")


echo "Env vars are:"
echo "PROJECT_ID is $PROJECT_ID"
echo "SERVICE_NAME is $SERVICE_NAME"
echo "CREDS is $CREDS"
echo "SERVICE_ACCOUNT_EMAIL is $SERVICE_ACCOUNT_EMAIL"
echo "CLOUD_RUN_SERVICE_URL is $CLOUD_RUN_SERVICE_URL"

echo "Getting token for service account in credentials file"
eval python3 get_token.py $CREDS