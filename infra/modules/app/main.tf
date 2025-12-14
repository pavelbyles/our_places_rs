
# Enable Cloud Run API service
resource "google_project_service" "cloud_run_api" {
  service = "run.googleapis.com"
}

resource "google_cloudrun_service" "service" {
  name     = "${var.env_prefix}-${var.service_name}"
  location = var.region
  project  = "${var.base_project_id}-${var.env_prefix}"
  template {
    spec {
      containers {
        image = "gcr.io/your-project-id/${var.env_prefix}-${var.service_name}:${var.image_tag}"
        env_vars = var.env_vars
      }
    }
  }
}
