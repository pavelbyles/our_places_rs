local_build:
	@echo "Creating image locally"
	@docker build --progress plain -t gcr.io/our-places-rs/our-places-web-app .

local_run:
	#@docker rm our-places-webapp-nextjs-container
	# @docker run -it -p 3000:3000 --name our-places-webapp-nextjs-container our-places-webapp-nextjs-image
	
	@npm run dev
	#npx next dev -p 3000

build_clean:
	@docker image prune -f

dev:
	@docker build -t our-places-webapp-nextjs-image .
	@echo "API image created: our-places-webapp-nextjs-image"

	# Set enironment vars
	@COMPOSE_DOCKER_CLI_BUILD=1
	@DOCKER_BUILDKIT=1

	# Run docker
	@docker compose build
	@docker compose up

local_stop:
	@docker stop our-places-web-app-nextjs-container