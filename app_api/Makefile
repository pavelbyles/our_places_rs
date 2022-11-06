dev:
	@echo "Building app_api dev build"
	@cargo build
	@echo "Copying config to target path..."
	@cp -R config/ target/debug/
	@echo "... completed copying to target path"
	@echo "Completed app_api dev build"

release:
	@echo "Building app_api release build"
	@cargo build --release
	@echo "Copying config to target path..."
	@cp -R config/ target/release/
	@echo "... completed copying to target path"
	@echo "Completed release build"

deploy:
	gcloud run deploy our-places-app-api-rs --source . --allow-unauthenticated --max-instances 1 --memory 256Mi

clean:
	cargo clean
