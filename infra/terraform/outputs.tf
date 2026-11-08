output "resource_group_name" {
  value = azurerm_resource_group.main.name
}

output "vm_name" {
  value = azurerm_linux_virtual_machine.k3s.name
}

output "vm_id" {
  value = azurerm_linux_virtual_machine.k3s.id
}

output "vm_size" {
  value = azurerm_linux_virtual_machine.k3s.size
}

output "public_ip" {
  value = azurerm_public_ip.k3s.ip_address
}

output "public_fqdn" {
  description = "Nombre DNS público para el Ingress y el certificado TLS."
  value       = azurerm_public_ip.k3s.fqdn
}

output "ssh_command" {
  value = "ssh ${var.admin_username}@${azurerm_public_ip.k3s.fqdn}"
}

output "kubernetes_api_tunnel" {
  description = "La API de Kubernetes no se expone: se accede por un túnel SSH."
  value       = "ssh -N -L 6443:127.0.0.1:6443 ${var.admin_username}@${azurerm_public_ip.k3s.fqdn}"
}

output "k3s_version" {
  value = var.k3s_version
}
