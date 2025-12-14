variable "env_prefix" {}
variable "region" {}

#resource "google_compute_network" "vpc_network" {
#  name                    = "vpc-${var.env_prefix}"
#  auto_create_subnetworks = false
#}

#resource "google_compute_subnetwork" "subnet_a" {
#  name          = "${var.env_prefix}-subnet-a"
#  ip_cidr_range = "10.0.0.0/24"
#  region        = var.region
#  network       = google_compute_network.vpc_network.id
#  private_ip_google_access = true
#}

#resource "google_compute_address" "ourplaces-dev-vpc-ip-range" {
#  name         = "psc-compute-address"
#  region       = var.region
#  address_type = "INTERNAL"
#  subnetwork   = google_compute_subnetwork.subnet_a.name
#  address      = "10.0.0.1"
#}

#data "google_sql_database_instance" "ourplaces_db_instance_data" {
#  name = resource.google_sql_database_instance.ourplaces_db_instance.name
#}

#resource "google_compute_forwarding_rule" "default" {
#  name                  = "psc-forwarding-rule-ourplaces-db"
#  region                = var.region
#  network               = google_compute_network.network.name
#  ip_address            = google_compute_address.ourplaces-dev-vpc-ip-range.self_link
#  load_balancing_scheme = ""
#  target                = data.google_sql_database_instance.ourplaces_db_instance_data.psc_service_attachment_link
#}

#resource "google_compute_global_address" "private_ip_range" {
#  name          = "${var.env_prefix}-private-ip-range"
#  address_type  = "INTERNAL"
#  prefix_length = 24
#  purpose = "PRIVATE_SERVICE_CONNECT"
#  network = google_compute_network.network.id
#  address = "10.0.0.0"
#}

#resource "google_service_networking_connection" "service_networking" {
#  network                 = google_compute_network.network.id
#  reserved_peering_ranges  = [google_compute_address.ourplaces-dev-vpc-ip-range.address]
#  service                 = "services/servicenetworking.googleapis.com"
#}

