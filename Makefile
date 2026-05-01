.PHONY: up down logs ps build fmt test reset-db supervisor-shell backend-shell help

help:
	@echo "Stellarator Docker Orchestration"
	@echo ""
	@echo "Usage:"
	@echo "  make up                 Start all services"
	@echo "  make down               Stop all services"
	@echo "  make logs               View live logs"
	@echo "  make ps                 List running containers"
	@echo "  make build              Build all images"
	@echo "  make fmt                Format code (Python + JavaScript)"
	@echo "  make test               Run all tests"
	@echo "  make reset-db           Reset database volume"
	@echo "  make supervisor-shell   Open supervisor container shell"
	@echo "  make backend-shell      Open backend container shell"

up:
	docker compose up -d

down:
	docker compose down

logs:
	docker compose logs -f

ps:
	docker compose ps

build:
	docker compose build

fmt:
	docker compose exec backend black /app
	docker compose exec backend isort /app
	docker compose exec frontend npx prettier --write .

test:
	docker compose exec backend pytest
	docker compose exec frontend npm test

reset-db:
	docker compose down
	docker volume rm stellarator-data || true
	docker compose up -d

supervisor-shell:
	docker compose exec supervisor /bin/sh

backend-shell:
	docker compose exec backend /bin/bash

obs-up:
	docker compose --profile observability up -d

obs-down:
	docker compose --profile observability down
