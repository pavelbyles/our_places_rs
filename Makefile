MAKE := make

dev_booking_api:
	$(MAKE) -C app_api/booking_api dev

dev_listing_api:
	$(MAKE) -C app_api/listing_api dev

dev_admin_api:
	$(MAKE) -C admin_api dev

dev: dev_app_api dev_admin_api

release:
	$(MAKE) -C app_api release

deploy:
	$(MAKE) -C app_api deploy

clean:
	$(MAKE) -C app_api clean
