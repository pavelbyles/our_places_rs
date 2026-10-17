# Cloud Scheduler configuration for transactional messaging sweeps

resource "google_project_service" "cloud_scheduler_api" {
  project            = var.project_id
  service            = "cloudscheduler.googleapis.com"
  disable_on_destroy = false
}

resource "google_cloud_scheduler_job" "scheduled_notifications_sweep" {
  name             = "${var.env_prefix}-scheduled-notifications-sweep"
  description      = "Hourly sweep for 48h pre-arrival guides, 48h host arrival reminders, and 2h expiring hold warnings"
  project          = var.project_id
  region           = var.region
  schedule         = "0 * * * *"
  time_zone        = "UTC"
  attempt_deadline = "320s"

  retry_config {
    retry_count = 3
  }

  http_target {
    http_method = "POST"
    uri         = "${var.booking_api_url}/api/v1/internal/cron/process-scheduled-notifications"

    headers = {
      "Content-Type"  = "application/json"
      "x-cron-secret" = var.cron_secret
    }
  }

  depends_on = [google_project_service.cloud_scheduler_api]
}

resource "google_cloud_scheduler_job" "complete_stays" {
  name             = "${var.env_prefix}-complete-stays"
  description      = "Daily sweep at 12:00 PM Jamaica time to transition past-checkout bookings to Completed and issue GCT tax invoices"
  project          = var.project_id
  region           = var.region
  schedule         = "0 12 * * *"
  time_zone        = "America/Jamaica"
  attempt_deadline = "320s"

  retry_config {
    retry_count = 3
  }

  http_target {
    http_method = "PATCH"
    uri         = "${var.booking_api_url}/api/v1/internal/cron/complete-stays"

    headers = {
      "Content-Type"  = "application/json"
      "x-cron-secret" = var.cron_secret
    }
  }

  depends_on = [google_project_service.cloud_scheduler_api]
}
