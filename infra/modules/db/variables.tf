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
  type = string
  description = "SSD or HDD" 
}

variable "db_backup_time" {
  type = string
}

variable "db_backup_location" {
  type = string
}

variable "db_password" {
  type = string
  sensitive = true
}

variable "db_app_api_password" {
  type      = string
  sensitive = true
}
