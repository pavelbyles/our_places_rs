MAKE := make

dev_app_api:
	$(MAKE) -C app_api dev

dev_admin_api:
	$(MAKE) -C admin_api dev

dev: dev_app_api dev_admin_api

release:
	$(MAKE) -C app_api release

deploy:
	$(MAKE) -C app_api deploy

clean:
	$(MAKE) -C app_api clean
