variable "project" {}

resource "google_service_account" "service_account_build" {
  account_id   = "sa-build-deploy"  # The ID of the service account
  display_name = "Service account - Build and Deploy"   # Optional display name
  description  = "Service account to build and deploy OurPlaces.io app"  # Optional description
}

resource "google_project_iam_member" "cloud_run_admin_to_service_account_build" {
  project = var.project
  role    = "roles/run.admin"
  member  = "serviceAccount:${google_service_account.service_account_build.email}"  # Reference the service account's email
}


resource "google_project_iam_member" "storage_admin_to_service_account_build" {
  project = var.project
  role    = "roles/storage.admin"
  member  = "serviceAccount:${google_service_account.service_account_build.email}"  # Reference the service account's email
}
