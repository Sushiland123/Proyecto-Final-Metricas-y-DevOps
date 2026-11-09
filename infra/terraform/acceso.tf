# ---------------------------------------------------------------- acceso del equipo

# Claves públicas de los integrantes que administran el clúster: una por archivo
# en infra/ssh/<nombre>.pub, agregadas o quitadas por PR. La IP de origen de cada
# integrante no va en el repositorio: se agrega a admin_cidrs (secret
# INFRA_ADMIN_CIDRS y terraform.tfvars local).
locals {
  team_ssh_keys = {
    for f in fileset("${path.module}/../ssh", "*.pub") :
    trimsuffix(f, ".pub") => trimspace(file("${path.module}/../ssh/${f}"))
  }
}

# La clave del administrador se entrega al crear la VM (admin_ssh_key), y
# cambiar ese bloque reemplaza la VM. Las del equipo se escriben con la
# extensión CustomScript, que se actualiza en sitio: authorized_keys queda con
# exactamente la clave del administrador y las de infra/ssh. Azure solo admite
# cambiar extensiones con la VM encendida.
resource "azurerm_virtual_machine_extension" "team_ssh_keys" {
  name                       = "team-ssh-keys"
  virtual_machine_id         = azurerm_linux_virtual_machine.k3s.id
  publisher                  = "Microsoft.Azure.Extensions"
  type                       = "CustomScript"
  type_handler_version       = "2.1"
  auto_upgrade_minor_version = true
  tags                       = local.tags

  protected_settings = jsonencode({
    script = base64encode(templatefile("${path.module}/authorized-keys.sh.tftpl", {
      admin_username = var.admin_username
      admin_key      = trimspace(var.admin_ssh_public_key)
      team_keys      = local.team_ssh_keys
    }))
  })

  lifecycle {
    precondition {
      condition = alltrue([
        for key in values(local.team_ssh_keys) :
        can(regex("^(ssh-ed25519|ssh-rsa|ecdsa-sha2-nistp256) [A-Za-z0-9+/=]+( [^\\n]*)?$", key))
      ])
      error_message = "Cada archivo infra/ssh/*.pub debe contener una sola clave pública SSH (ssh-ed25519, ssh-rsa o ecdsa)."
    }
  }
}
