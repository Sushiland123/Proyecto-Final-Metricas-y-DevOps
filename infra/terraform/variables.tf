variable "subscription_id" {
  description = "Suscripción de Azure del proyecto."
  type        = string
}

variable "location" {
  description = "Región de Azure. mexicocentral es la región validada en la clase de integración con K3s."
  type        = string
  default     = "mexicocentral"
}

variable "name_suffix" {
  description = "Sufijo generado por el bootstrap para nombres globalmente únicos."
  type        = string

  validation {
    condition     = can(regex("^[a-z0-9]{3,8}$", var.name_suffix))
    error_message = "name_suffix debe tener entre 3 y 8 caracteres en minúscula o dígitos."
  }
}

variable "vm_size" {
  description = "Tamaño de la VM que aloja K3s, la aplicación, PostgreSQL y la observabilidad."
  type        = string
  default     = "Standard_B2s_v2"
}

variable "vm_zone" {
  description = "Zona de disponibilidad de la VM. En mexicocentral, Standard_B2s_v2 está restringido en la zona 2 para Azure for Students."
  type        = string
  default     = "1"
}

variable "os_disk_size_gb" {
  description = "Tamaño del disco del sistema; también aloja los volúmenes local-path de K3s."
  type        = number
  default     = 64
}

variable "admin_username" {
  description = "Usuario administrador de la VM."
  type        = string
  default     = "azureuser"
}

variable "admin_ssh_public_key" {
  description = "Clave pública SSH del administrador. No se permite autenticación por contraseña."
  type        = string

  validation {
    condition     = can(regex("^(ssh-ed25519|ssh-rsa|ecdsa-sha2-nistp256) ", var.admin_ssh_public_key))
    error_message = "admin_ssh_public_key debe ser una clave pública SSH (ssh-ed25519, ssh-rsa o ecdsa)."
  }
}

variable "admin_cidrs" {
  description = "Rangos IPv4 autorizados para SSH (por ejemplo la IP pública del administrador con /32)."
  type        = list(string)
  # El repositorio es público: la IP personal no debe aparecer en planes, logs ni artefactos.
  sensitive = true

  validation {
    # Solo IPv4 con prefijo /24 o más estrecho: también rechaza rangos abiertos
    # como 0.0.0.0/1. try() evita errores con entradas mal formadas.
    condition = length(var.admin_cidrs) > 0 && alltrue([
      for cidr in var.admin_cidrs :
      can(regex("^([0-9]{1,3}[.]){3}[0-9]{1,3}/[0-9]{1,2}$", cidr)) &&
      can(cidrhost(cidr, 0)) &&
      try(tonumber(split("/", cidr)[1]) >= 24, false)
    ])
    error_message = "admin_cidrs debe contener rangos IPv4 de /24 o más estrechos (por ejemplo la IP del administrador con /32)."
  }
}

variable "k3s_version" {
  description = "Versión fija de K3s que instala cloud-init."
  type        = string
  default     = "v1.35.2+k3s1"
}

variable "create_budget" {
  description = "Crear el presupuesto mensual con alertas a nivel de suscripción."
  type        = bool
  default     = true
}

variable "budget_amount" {
  description = "Monto mensual del presupuesto en USD (el límite del proyecto es 100)."
  type        = number
  default     = 100
}

variable "budget_start_date" {
  description = "Inicio del período del presupuesto: primer día de un mes, en UTC."
  type        = string
  default     = "2026-11-01T00:00:00Z"
}

variable "budget_contact_emails" {
  description = "Correos que reciben las alertas del presupuesto."
  type        = list(string)
  sensitive   = true
  default     = []
}
