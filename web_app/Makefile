local_build:
	@npm run build	

local_run:
	@npm run dev --show-all

dev_build:
	@docker build --progress plain -t gcr.io/our-places-rs/our-places-web-app .
	@echo "API image created: gcr.io/our-places-rs/our-places-web-app"

build_clean:
	@docker image prune -f

dev_run:
	@docker run -it -p 8080:8080 gcr.io/our-places-rs/our-places-web-app