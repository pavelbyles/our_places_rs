env_prefix = "prd"
region = "us-central1"
image_tag = "our-places:prd"

db_tier = "db-f1-micro"
db_edition = "ENTERPRISE"
db_availability_type = "ZONAL"

db_disk_type = "PD_SSD"
db_disk_size = 10
db_disk_autoresize = true 

db_backup_time = "02:00"
db_backup_location = "us"
