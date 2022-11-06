gcloud dns --project=our-places-dev managed-zones create ourplaces-dev-api-zone --description="" --dns-name="api.dev.ourplaces.io." --visibility="private" --networks="default"

# Generate certificate for API's
gcloud compute ssl-certificates create ourplaces-apicertdev \
    --description="Certificate for dev apis" \
    --domains=dev.api.ourplaces.io \
    --global

# TF version
resource "google_compute_managed_ssl_certificate" "lb_default" {
  provider = google-beta
  name     = "ourplaces-apicertdev"

  managed {
    domains = [dev.api.ourplaces.io]
  }
}


# List certs
gcloud compute ssl-certificates list \
   --global