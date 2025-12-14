variable "base_project_id" {
  type        = string
  description = "Base GCP Project ID"
}

variable "project_name" {
  type        = string
  description = "The cancnonical name of the project (not the project id)"
  default     = "ourplaces"
}

variable "env_prefix" {
  type        = string
  description = "Environment Prefix (dev, uat, prod)"
}

variable "region" {
  type        = string
  description = "GCP Region"
}

variable "image_tag" {
  type        = string
  description = "Docker Image Tag"
}

variable "db_edition" {
  type = string
}

variable "db_tier" {
  type = string
}

variable "db_availability_type" {
  type = string
}

variable "db_disk_size" {
  type = number
}

variable "db_disk_autoresize" {
  type = bool
}

variable "db_disk_type" {
  type        = string
  description = "SSD or HDD"
}

variable "db_backup_time" {
  type = string
}

variable "db_backup_location" {
  type = string
}

variable "db_password" {
  type      = string
  sensitive = true
}

variable "db_app_api_password" {
  type      = string
  sensitive = true
}
