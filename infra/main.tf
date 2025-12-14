terraform {
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "6.13.0"
    }
  }
}

provider "google" {
  project = var.base_project_id
  region  = var.region
}

module "network" {
  source     = "./modules/network"
  env_prefix = var.env_prefix
  region     = var.region
}

module "db" {
  source               = "./modules/db"
  env_prefix           = var.env_prefix
  region               = var.region
  project_name         = var.project_name
  db_edition           = var.db_edition
  db_tier              = var.db_tier
  db_availability_type = var.db_availability_type
  db_disk_type         = var.db_disk_type
  db_disk_size         = var.db_disk_size
  db_backup_time       = var.db_backup_time
  db_backup_location   = var.db_backup_location
  db_disk_autoresize   = var.db_disk_autoresize
  db_password          = var.db_password
  db_app_api_password  = var.db_app_api_password

  # Network params
  #private_network      = module.network.network_id
}

module "accounts" {
  source = "./modules/accounts"
  project = var.base_project_id
}
