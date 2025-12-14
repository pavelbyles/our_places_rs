terraform {
  backend "gcs" {
    bucket = "ourplaces-terraform-state"
  }
}
