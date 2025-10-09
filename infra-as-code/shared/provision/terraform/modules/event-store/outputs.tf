output "service_ports" {
  description = "Service ports used by the event store (grpc/http/metrics)."
  value       = local.ports
}

output "service_environment" {
  description = "Environment variables used to configure the event store service."
  value       = local.service_environment
}

output "default_tags" {
  description = "Standard tags to apply to provisioned resources."
  value       = local.tags
}
