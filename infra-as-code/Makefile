PYTHON?=python3
TOOLS_DIR?=$(CURDIR)/tools

render-config:
	$(PYTHON) $(TOOLS_DIR)/render_config.py --target $(TARGET) --env $(ENV)

aws-render-prod:
	TARGET=aws ENV=prod $(MAKE) render-config

proxmox/provision/config/local.yml: .env
	./generate-config.sh

proxmox-render-local: proxmox/provision/config/local.yml
	TARGET=proxmox ENV=local $(MAKE) render-config

aws-terraform-plan:
	cd aws/provision/terraform/envs/prod && terraform plan -var-file=terraform.tfvars.json

aws-terraform-apply:
	cd aws/provision/terraform/envs/prod && terraform apply -var-file=terraform.tfvars.json

aws-ansible-configure:
	ANSIBLE_ROLES_PATH=$(CURDIR)/shared/configure/ansible/roles \
	ansible-playbook -i aws/configure/ansible/envs/prod/inventory.ini aws/configure/ansible/envs/prod/playbook.yml

proxmox-terraform-plan: proxmox-render-local
	cd proxmox/provision/terraform/envs/local && terraform plan -var-file=terraform.tfvars.json

proxmox-terraform-apply: proxmox-render-local
	cd proxmox/provision/terraform/envs/local && terraform apply -auto-approve -var-file=terraform.tfvars.json

proxmox-ansible-configure:
	ANSIBLE_ROLES_PATH=$(CURDIR)/shared/configure/ansible/roles \
	ansible-playbook -i proxmox/configure/ansible/envs/local/inventory.ini proxmox/configure/ansible/envs/local/playbook.yml

# Docker image targets
docker-build-amd64:
	@echo "🐳 Building Event Store Docker image for x86_64..."
	cd .. && docker buildx build --platform linux/amd64 \
		-t event-sourcing-platform-event-store:latest \
		-f event-store/Dockerfile \
		--load .

docker-save:
	@echo "💾 Saving Docker image..."
	docker save event-sourcing-platform-event-store:latest | gzip > /tmp/event-store-amd64.tar.gz
	@ls -lh /tmp/event-store-amd64.tar.gz

# Gateway image (ADR-024) - the only component this deployment publishes to
# the host. See event-store/gateway/README.md.
docker-build-gateway-amd64:
	@echo "🐳 Building Event Store Gateway Docker image for x86_64..."
	cd ../event-store/gateway && docker buildx build --platform linux/amd64 \
		-t event-sourcing-platform-gateway:latest \
		--load .

docker-save-gateway:
	@echo "💾 Saving Gateway image..."
	docker save event-sourcing-platform-gateway:latest | gzip > /tmp/gateway-amd64.tar.gz
	@ls -lh /tmp/gateway-amd64.tar.gz

docker-build-and-save: docker-build-amd64 docker-save docker-build-gateway-amd64 docker-save-gateway
	@echo "✅ Docker images built and saved to /tmp/event-store-amd64.tar.gz and /tmp/gateway-amd64.tar.gz"

# Full deployment workflow
proxmox-deploy-full: proxmox-render-local docker-build-and-save proxmox-terraform-apply proxmox-ansible-configure
	@echo ""
	@echo "🎉 Deployment complete!"
	@echo ""
	@echo "📊 Connection info (ADR-024: all traffic goes through the auth gateway):"
	@echo "   Event Store: 192.168.0.100:50051 (Basic Auth required - see ESP_GATEWAY_PASSWORD)"
	@echo ""
	@echo "🧪 Test with grpcurl - see event-store/gateway/README.md 'Verify' section"
	@echo "   for the exact command (avoids base64 line-wrapping issues in a"
	@echo "   shell-portable way)."
	@echo ""
	@echo "⚠️  KNOWN GAP: the Rust SDK supports Basic Auth (ClientConfig::basic_auth);"
	@echo "   the TS/Python SDK clients do not yet (#302) and will fail against"
	@echo "   this gateway-protected deployment. See event-store/gateway/README.md."

proxmox-destroy:
	@echo "🗑️  Destroying Proxmox VM..."
	cd proxmox/provision/terraform/envs/local && terraform destroy -var-file=terraform.tfvars.json

.PHONY: render-config aws-render-prod proxmox-render-local \
	aws-terraform-plan aws-terraform-apply aws-ansible-configure \
	proxmox-terraform-plan proxmox-terraform-apply proxmox-ansible-configure \
	docker-build-amd64 docker-save docker-build-and-save \
	proxmox-deploy-full proxmox-destroy
