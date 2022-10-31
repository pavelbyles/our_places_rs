debug:
	@cargo build
	cp -R config/ target/debug/

release:
	@cargo build --release
	cp -R config/ target/release/

deploy:
	@gcloud run deploy our-places-app-api-rs --source . --allow-unauthenticated --max-instances 1 --memory 256Mi