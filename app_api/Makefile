local_build:
	@echo "Creating image locally"
	@docker build -t our-places-app-api-rs-image .
	@echo "API image created: our-places-app-api-rs-image"

local_run:
	@docker rm our-places-app-api-rs-container
	@docker run -it -p 8080:8080 --name our-places-app-api-rs-container our-places-app-api-rs-image

local_stop:
	@docker stop our-places-app-api-rs-container

dev:
	@echo "Run tests"
	@cargo test
	@echo "Building app_api dev build"
	@cargo make clean-build
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
	@gcloud run deploy our-places-app-api-rs --region us-central1 --source . --allow-unauthenticated --max-instances 1 --memory 256Mi

clean:
	cargo clean
