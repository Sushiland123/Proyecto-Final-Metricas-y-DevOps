variable "subscription_id" {
  description = "Suscripción de Azure del proyecto."
  type        = string
}

variable "location" {
  description = "Región de Azure. Debe estar permitida por la política de la suscripción y ofrecer el tamaño de VM elegido."
  type        = string
  default     = "westus"
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
  default     = "Standard_B2as_v2"
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

  validation {
    condition = length(var.admin_cidrs) > 0 && alltrue([
      for cidr in var.admin_cidrs : can(cidrhost(cidr, 0)) && !contains(["0.0.0.0/0", "*"], cidr)
    ])
    error_message = "admin_cidrs debe contener rangos CIDR válidos y nunca 0.0.0.0/0."
  }
}

variable "k3s_version" {
  description = "Versión fija de K3s que instala cloud-init."
  type        = string
  default     = "v1.35.2+k3s1"
}

variable "auto_shutdown_time" {
  description = "Hora diaria (HHmm) en que la VM se apaga y desasigna para dejar de cobrar cómputo."
  type        = string
  default     = "2300"

  validation {
    condition     = can(regex("^([01][0-9]|2[0-3])[0-5][0-9]$", var.auto_shutdown_time))
    error_message = "auto_shutdown_time debe tener formato HHmm."
  }
}

variable "auto_shutdown_timezone" {
  description = "Zona horaria del apagado automático."
  type        = string
  default     = "Central America Standard Time"
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
  default     = []
}
