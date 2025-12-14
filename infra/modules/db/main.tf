variable "env_prefix" {}
variable "region" {}
variable "project_name" {}
#variable "private_network" {}

resource "google_sql_database" "ourplaces_db" {
  name = "${var.project_name}-db-${var.env_prefix}"
  instance = google_sql_database_instance.ourplaces_db_instance.name
}

resource "google_sql_database_instance" "ourplaces_db_instance" {
  name        = "${var.project_name}-${var.env_prefix}"
  database_version = "POSTGRES_17"
  region      = var.region
  root_password = var.db_password

  settings {
    tier = var.db_tier
    edition = var.db_edition
    availability_type = var.db_availability_type
    disk_type = var.db_disk_type
    disk_size = var.db_disk_size
    disk_autoresize = var.db_disk_autoresize

    backup_configuration {
      start_time = var.db_backup_time
      location    = var.db_backup_location
    }

    # IP Configuration for Private IP
    #ip_configuration {
    #  private_network = var.private_network  # Reference the VPC
    #  ipv4_enabled    = false  # Disable publicly accessible IP
    #}
  }
}

resource "google_sql_user" "users" {
  name     = "api_usr"
  instance = google_sql_database_instance.ourplaces_db_instance.name
  password = var.db_app_api_password 
}
