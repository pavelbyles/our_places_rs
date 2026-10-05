variable "project_id" {
  type        = string
  description = "GCP Project ID"
}

variable "region" {
  type        = string
  description = "GCP Region for Cloud Scheduler"
}

variable "env_prefix" {
  type        = string
  description = "Environment prefix (dev, uat, prod)"
}

variable "booking_api_url" {
  type        = string
  description = "Base URL of booking_api Cloud Run service"
}

variable "cron_secret" {
  type        = string
  description = "Shared secret for authenticating internal cron trigger sweeps"
  sensitive   = true
}
