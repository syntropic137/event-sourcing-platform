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

docker-build-and-save: docker-build-amd64 docker-save
	@echo "✅ Docker image built and saved to /tmp/event-store-amd64.tar.gz"

# Full deployment workflow
proxmox-deploy-full: proxmox-render-local docker-build-and-save proxmox-terraform-apply proxmox-ansible-configure
	@echo ""
	@echo "🎉 Deployment complete!"
	@echo ""
	@echo "📊 Connection info:"
	@echo "   Event Store: 192.168.0.100:50051"
	@echo ""
	@echo "🧪 Test with:"
	@echo "   EVENT_STORE_ADDR=192.168.0.100:50051 npm run start --prefix ../examples/002-simple-aggregate-ts"

proxmox-destroy:
	@echo "🗑️  Destroying Proxmox VM..."
	cd proxmox/provision/terraform/envs/local && terraform destroy -var-file=terraform.tfvars.json

.PHONY: render-config aws-render-prod proxmox-render-local \
	aws-terraform-plan aws-terraform-apply aws-ansible-configure \
	proxmox-terraform-plan proxmox-terraform-apply proxmox-ansible-configure \
	docker-build-amd64 docker-save docker-build-and-save \
	proxmox-deploy-full proxmox-destroy
